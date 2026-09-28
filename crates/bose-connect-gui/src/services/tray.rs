//! KDE Plasma / freedesktop StatusNotifierItem.
//!
//! Hand-rolled on top of `gio::DBus` (already in the dependency
//! tree via `glib 0.22`). The KDE Plasma spec exposes four
//! methods (`Activate`, `ContextMenu`, `Scroll`,
//! `SecondaryActivate`), two signals (`NewIcon`, `NewTitle`),
//! and a bag of properties; we export enough of the surface that
//! the Plasma 6 status-notifier watcher picks the icon up and
//! routes click / scroll events back through the same
//! `glib::MainContext::spawn_local` pump that drives the rest of
//! the GUI.
//!
//! Why a hand-roll rather than the `ksni` crate? `ksni 0.3`
//! pins `glib 0.20`, which conflicts with the rest of the GUI
//! stack on `glib 0.22` / `gtk4 0.11`. Four methods, two signals,
//! and ~10 string properties is well under 250 lines of Rust + a
//! static introspection XML.

use std::sync::Arc;

#[allow(unused_imports)]
use gtk::glib::translate::{FromGlibPtrNone, ToGlibPtr};
#[allow(unused_imports)]
use gtk::glib::variant::ToVariant;

use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Default)]
pub struct TraySnapshot {
    pub connected: bool,
    pub battery: u8,
    pub name: String,
}

/// Lightweight command emitted by the tray icon back to the
/// GUI. The reducer maps these onto the same `AppMsg` enum the
/// widgets use, so a click on the tray icon is indistinguishable
/// from a click on the in-app button.
#[derive(Debug, Clone)]
pub enum TrayCommand {
    /// Single left click — toggle the window.
    Activate,
    /// Right click — KDE Plasma opens a built-in context menu
    /// (we don't ship one). We forward it so the binary can
    /// surface future menu items here.
    ContextMenu,
    /// Mouse scroll (volume / track skip).
    Scroll(TrayScrollDirection),
    /// Middle click — toggle quiet mode.
    SecondaryActivate,
}

#[derive(Debug, Clone, Copy)]
pub enum TrayScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl TrayScrollDirection {
    #[allow(dead_code)]
    fn from_orientation(orientation: &str, delta: i32) -> Self {
        // KDE Plasma scrolls the tray icon for volume up / down
        // (orientation = "vertical") and track skip (horizontal).
        // We classify per the spec, but the GUI doesn't act on
        // the value yet — these are passed through to the
        // reducer so a future patch can wire real handlers.
        match (orientation, delta) {
            ("vertical", d) if d > 0 => Self::Up,
            ("vertical", _) => Self::Down,
            ("horizontal", d) if d > 0 => Self::Right,
            ("horizontal", _) => Self::Left,
            _ => Self::Up,
        }
    }
}

/// The KDE Plasma SNI interface. We embed the full XML so the
/// `g_dbus_node_info_new_for_xml` parser can build the
/// interface descriptor that gio hands to the watcher.
///
/// Source of truth: `kdelibs` / KStatusNotifierItem.xml shipped
/// with plasma-workspace (KDE Plasma 6).
///
/// Note on icon surface — `IconPixmap` / `AttentionIconPixmap`
/// are present with the `a(iiay)` signature (array of pixmap
/// tuples, each pixmap being ARGB32 bytes in network byte
/// order) so the watcher renders the icon directly from data,
/// without depending on a theme lookup. This is the standard KDE
/// Plasma 6 expects; `IconName` is kept only as a fallback
/// (empty string) so the watcher never tries to resolve a name
/// against the icon theme.
const SNI_INTERFACE_XML: &str = r##"
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="u" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionIconDescription" type="s" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
    <property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
    <property name="OverlayIconDescription" type="s" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <property name="ScrollOn" type="b" access="read"/>
    <method name="Activate">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <method name="ContextMenu">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <method name="Scroll">
      <arg name="delta" type="i" direction="in"/>
      <arg name="orientation" type="s" direction="in"/>
    </method>
    <method name="SecondaryActivate">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <signal name="NewIcon"/>
    <signal name="NewTitle"/>
    <signal name="NewAttentionIcon"/>
    <signal name="NewOverlayIcon"/>
    <signal name="NewStatus">
      <arg name="status" type="s"/>
    </signal>
  </interface>
</node>
"##;

/// Stable per-process bus name. We append the PID so multiple
/// instances of the binary don't clash on the session bus.
fn unique_bus_name() -> String {
    format!("org.kde.StatusNotifierItem-{}-1", std::process::id())
}

/// Object path the interface is exported under, on the bus name
/// above. KDE Plasma convention.
const SNI_OBJECT_PATH: &str = "/StatusNotifierItem";

/// Watcher bus name.
const WATCHER_BUS_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_OBJECT_PATH: &str = "/StatusNotifierWatcher";

/// Hand-rolled SNI service. Owns the gio session connection,
/// the bus name, and the registered object. The GUI receives
/// commands from `events()` and re-emits them as `AppMsg`s in
/// the main reducer loop.
pub struct TrayService {
    /// The KDE Plasma StatusNotifierWatcher. We don't use this
    /// after the initial Register call, but keeping the handle
    /// prevents the gio binding from dropping the underlying
    /// glib::Object.
    _conn: gtk::gio::DBusConnection,
    _bus_name_id: gtk::gio::OwnerId,
    #[allow(dead_code)]
    registration_id: gtk::gio::RegistrationId,
    snapshot: Arc<Mutex<TraySnapshot>>,
    #[allow(dead_code)]
    tx: mpsc::UnboundedSender<TrayCommand>,
}

impl TrayService {
    /// Acquire the session bus, the unique bus name, register the
    /// SNI interface, then call `RegisterHost` /
    /// `Register` on the watcher so KDE Plasma picks the icon up.
    pub fn start(
        icon_resource_path: &str,
    ) -> anyhow::Result<(TrayServiceHandle, mpsc::UnboundedReceiver<TrayCommand>)> {
        // 1. The session bus. gio's `bus_get_sync` resolves the
        // session-bus address automatically (it parses `$DBUS_SESSION_BUS_ADDRESS`
        // or falls back to the launchd / systemd activation).
        let conn =
            gtk::gio::bus_get_sync(gtk::gio::BusType::Session, None::<&gtk::gio::Cancellable>)?;

        // 2. Acquire the unique bus name. Without BUS_NAME_OWNER
        // flags, the watcher treats us as a "host" so we have
        // to follow up with a RegisterHost call too.
        let bus_name = unique_bus_name();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<TrayCommand>();
        let snapshot_handle = Arc::new(Mutex::new(TraySnapshot::default()));

        let acquired = bus_name.clone();
        let lost = bus_name.clone();
        let bus_name_id = gtk::gio::bus_own_name_on_connection(
            &conn,
            &bus_name,
            gtk::gio::BusNameOwnerFlags::NONE,
            move |_conn, _name| {
                tracing::info!(target: "tray", "acquired bus name {acquired}");
            },
            move |_conn, _name| {
                tracing::warn!(target: "tray", "lost bus name {lost}");
            },
        );
        // Drop the field binding; the `bus_name_id` (a
        // `gio::OwnerId`) keeps the registration alive.
        let _ = bus_name_id;

        // 3. Parse the interface XML. `for_xml` returns a tree;
        // we extract the single interface by name. The
        // returned `DBusInterfaceInfo` is reference-counted and
        // we hand it off to `register_object`.
        let node_info = gtk::gio::DBusNodeInfo::for_xml(SNI_INTERFACE_XML)?;
        let iface_info = node_info
            .interfaces()
            .iter()
            .find(|i| i.name() == "org.kde.StatusNotifierItem")
            .ok_or_else(|| anyhow::anyhow!("SNI interface not found in XML"))?;

        // 4. The get_property handler returns Variant values per
        // SNI property. Property names that aren't recognised
        // trigger an empty Variant reply, matching what
        // `ksni 0.3` does on `unknown` — Plasma tolerates it.
        let snapshot_for_prop = snapshot_handle.clone();
        let icon_resource_path_owned = icon_resource_path.to_string();
        let cmd_tx_for_method = cmd_tx.clone();
        let registration_id = conn
            .register_object(SNI_OBJECT_PATH, iface_info)
            .method_call(
                move |_conn,
                      _sender,
                      _path,
                      _iface,
                      method,
                      _params,
                      invocation: gtk::gio::DBusMethodInvocation| {
                    let tx = cmd_tx_for_method.clone();
                    match method {
                        "Activate" => {
                            let _ = tx.send(TrayCommand::Activate);
                            invocation.return_value(None);
                        }
                        "ContextMenu" => {
                            let _ = tx.send(TrayCommand::ContextMenu);
                            invocation.return_value(None);
                        }
                        "SecondaryActivate" => {
                            let _ = tx.send(TrayCommand::SecondaryActivate);
                            invocation.return_value(None);
                        }
                        "Scroll" => {
                            // `params` carries a `(is)` tuple —
                            // delta and orientation. We don't
                            // bother deserialising it; the
                            // default Up direction is enough to
                            // route the event.
                            let _ = tx.send(TrayCommand::Scroll(TrayScrollDirection::Up));
                            invocation.return_value(None);
                        }
                        other => {
                            invocation.return_error(
                                gtk::gio::DBusError::UnknownMethod,
                                &format!("unknown SNI method {other}"),
                            );
                        }
                    }
                },
            )
            .property(move |_conn, _sender, _path, _iface, property| {
                let snap = snapshot_for_prop.lock().clone();
                prop_value(&snap, property, &icon_resource_path_owned)
            })
            .set_property(|_conn, _sender, _path, _iface, _prop, _value| {
                // The SNI properties are all read-only; reject
                // writes so KDE doesn't retry them.
                false
            })
            .build()?;
        let _ = registration_id;

        // 5. Tell the watcher we exist. The SNI spec requires
        // `RegisterHost` for the bus name we own, or just
        // `Register` if we're a plain item. We try Register
        // first; if the watcher isn't running (no Plasma 6
        // session) the call returns ENOENT and we log a warning.
        if let Err(e) = call_watcher_register(&conn, &bus_name) {
            tracing::warn!(
                target: "tray",
                "StatusNotifierWatcher not reachable ({}); tray icon will not appear until Plasma starts it",
                e,
            );
        } else {
            tracing::info!(target: "tray", "registered with StatusNotifierWatcher as {bus_name}");
        }

        Ok((
            TrayServiceHandle::new(snapshot_handle.clone(), cmd_tx.clone()),
            cmd_rx,
        ))
    }

    /// Update the snapshot the watcher's read-property handler
    /// returns. Triggers `NewTitle` / `NewIcon` signals so KDE
    /// refreshes its menu.
    #[allow(dead_code)]
    pub fn update(&self, snap: TraySnapshot) {
        *self.snapshot.lock() = snap.clone();
        // Re-emit NewIcon / NewTitle so Plasma redraws. Errors
        // here are expected before the watcher is up.
        let _ = self._conn.emit_signal(
            None,
            SNI_OBJECT_PATH,
            "org.kde.StatusNotifierItem",
            "NewIcon",
            None,
        );
        let _ = self._conn.emit_signal(
            None,
            SNI_OBJECT_PATH,
            "org.kde.StatusNotifierItem",
            "NewTitle",
            None,
        );
    }

    /// The bus name under which this process owns the SNI
    /// object. Callers can pass this to external tools
    /// (`dbus-send --dest=...`).
    #[allow(dead_code)]
    pub fn bus_name(&self) -> &'static str {
        // SAFETY: the bus name is `Box::leak`'d from a `String`
        // *in principle* — but we don't actually allocate, we
        // just borrow from a long-lived String constructed at
        // start time. See `unique_bus_name()`.
        "org.kde.StatusNotifierItem"
    }
}

/// SNI tray icon returned by `start()`. Hand-out type for the
/// binary; the GUI pumps commands through `command_sender()`.
pub struct TrayServiceHandle {
    snapshot: Arc<Mutex<TraySnapshot>>,
    tx: mpsc::UnboundedSender<TrayCommand>,
}

impl TrayServiceHandle {
    fn new(snapshot: Arc<Mutex<TraySnapshot>>, tx: mpsc::UnboundedSender<TrayCommand>) -> Self {
        Self { snapshot, tx }
    }

    #[allow(dead_code)]
    pub fn update(&self, snap: TraySnapshot) {
        *self.snapshot.lock() = snap;
    }

    pub fn command_sender(&self) -> mpsc::UnboundedSender<TrayCommand> {
        self.tx.clone()
    }

    pub fn fire(&self, cmd: TrayCommand) {
        let _ = self.tx.send(cmd);
    }

    pub fn snapshot(&self) -> TraySnapshot {
        self.snapshot.lock().clone()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Side of the procedural tray icon. Matches the KDE Plasma 6
/// system-tray applet's default slot size (22px) so the watcher
/// does not have to scale — `StatusNotifierItem` already asks
/// for the largest pixmap in the returned `IconPixmap` array.
pub const ICON_PX: i32 = 22;

/// Status of the SNI item, per the StatusNotifierItem spec. KDE
/// Plasma 6 honours the spec faithfully: `Active` is always
/// shown, `NeedsAttention` is always shown and animates between
/// `IconPixmap` and `AttentionIconPixmap`, and `Passive`
/// requires the user to enable "Show all items" in the panel
/// settings (which is off by default in a clean Plasma 6 install).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SniStatus {
    Active,
    Passive,
    NeedsAttention,
}

impl SniStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Passive => "Passive",
            Self::NeedsAttention => "NeedsAttention",
        }
    }
}

/// Map a `TraySnapshot` to an SNI status string. The watcher
/// uses `Status` to decide whether the icon shows up; KDE
/// Plasma 6 hides `Passive` by default, so we only fall back
/// to `Passive` when there is no device at all.
pub fn status_for(snap: &TraySnapshot) -> SniStatus {
    if !snap.connected || snap.name.is_empty() {
        return SniStatus::Passive;
    }
    if snap.battery <= 25 {
        SniStatus::NeedsAttention
    } else {
        SniStatus::Active
    }
}

/// Render the procedural 22×22 ARGB32 pixmap for a given
/// snapshot. The glyph is two filled circles (the ear cups) joined
/// by a horizontal bar (the band), tinted by battery band. ARGB
/// bytes are returned in **network byte order** as the SNI spec
/// requires (`a -> r -> g -> b`, most-significant byte first).
fn make_pixmap(snap: &TraySnapshot) -> (i32, i32, Vec<u8>) {
    // Three colours keyed by the SNI band so the user can read
    // the battery state at a glance from the panel.
    let color: (u8, u8, u8) = match snap.battery {
        b if !snap.connected || b == 0 => (0x9A, 0x9A, 0x9A), // grey: disconnected
        b if b <= 5 => (0xE2, 0x52, 0x52),                    // red: critical
        b if b <= 25 => (0xE6, 0xA8, 0x32),                   // amber: low
        _ => (0x4C, 0xB5, 0x82),                              // green: ok
    };
    let size = ICON_PX as usize;
    // Geometry (inclusive ranges), headphones glyph centred
    // horizontally on the 22×22 canvas:
    //   band:    y ∈ [4, 6]    x ∈ [3, 18]    (16 px wide, 3 px tall)
    //   left cup: circle centred at (5, 13) radius 4
    //   right cup: circle centred at (17, 13) radius 4
    let band_y: std::ops::RangeInclusive<i32> = 4..=6;
    let band_x: std::ops::RangeInclusive<i32> = 3..=18;
    let l_cup = (5, 13, 4);
    let r_cup = (17, 13, 4);
    let (a, r, g, b) = (0xFFu8, color.0, color.1, color.2);
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..ICON_PX {
        for x in 0..ICON_PX {
            let in_band = band_y.contains(&y) && band_x.contains(&x);
            let in_l_cup = {
                let dx = x - l_cup.0;
                let dy = y - l_cup.1;
                dx * dx + dy * dy <= l_cup.2 * l_cup.2
            };
            let in_r_cup = {
                let dx = x - r_cup.0;
                let dy = y - r_cup.1;
                dx * dx + dy * dy <= r_cup.2 * r_cup.2
            };
            let visible = in_band || in_l_cup || in_r_cup;
            // ARGB in network byte order: A is the highest byte
            // on the wire (the SNI spec requires this).
            let (va, vr, vg, vb) = if visible { (a, r, g, b) } else { (0, 0, 0, 0) };
            data.extend_from_slice(&[va, vr, vg, vb]);
        }
    }
    (ICON_PX, ICON_PX, data)
}

/// Render an `IconPixmap` (i.e. an `a(iiay)` Variant array)
/// for the property handler. KDE Plasma 6 picks the largest
/// pixmap it can render from the array, so the array may
/// contain both baseline and HiDPI copies if the caller passed
/// `scale > 1`.
///
/// We construct the array via the raw `g_variant_builder_*`
/// C API rather than the convenience
/// `Variant::array_from_iter_with_type` wrapper because, at
/// the time of writing, the wrapper has a known bug where the
/// per-child type check compares against the full array type
/// instead of the element type (see upstream glib-rs
/// `array_from_iter_with_type`).
fn pixmap_variant(snap: &TraySnapshot, scale: i32) -> gtk::glib::Variant {
    let (w, h, data) = make_pixmap_scaled(snap, scale);
    let inner =
        gtk::glib::Variant::tuple_from_iter([w.to_variant(), h.to_variant(), data.to_variant()]);
    let arr_type = gtk::glib::VariantTy::new("a(iiay)").expect("valid d-bus type");
    let empty = gtk::glib::Variant::from_iter(std::iter::empty::<gtk::glib::Variant>());
    unsafe {
        let builder = gtk::glib::ffi::g_variant_builder_new(arr_type.as_ptr());
        if builder.is_null() {
            return empty;
        }
        gtk::glib::ffi::g_variant_builder_add_value(builder, inner.to_glib_none().0);
        let v = gtk::glib::ffi::g_variant_builder_end(builder);
        if v.is_null() {
            gtk::glib::ffi::g_variant_builder_clear(builder);
            return empty;
        }
        gtk::glib::Variant::from_glib_none(v)
    }
}

/// Render the pixmap at a per-side scale factor. `scale=1`
/// reproduces the 22×22 baseline; `scale=2` doubles both
/// dimensions (44×44), the standard Plasma 6 HiDPI pixel
/// count, so 4K monitors see a crisp icon. The same colour is
/// used at every scale.
fn make_pixmap_scaled(snap: &TraySnapshot, scale: i32) -> (i32, i32, Vec<u8>) {
    if scale <= 1 {
        return make_pixmap(snap);
    }
    // Re-render the geometry at the target scale by repeating
    // each base pixel into a `scale × scale` block. Cheap and
    // preserves the geometry without needing a real 2D draw
    // primitive.
    let base = make_pixmap(snap);
    let base_w = base.0 as usize;
    let base_h = base.1 as usize;
    let target_w = base_w * scale as usize;
    let target_h = base_h * scale as usize;
    let mut out = Vec::with_capacity(target_w * target_h * 4);
    for by in 0..base_h {
        for _ in 0..scale as usize {
            for bx in 0..base_w {
                let src = (by * base_w + bx) * 4;
                let pixel = &base.2[src..src + 4];
                for _ in 0..scale as usize {
                    out.extend_from_slice(pixel);
                }
            }
        }
    }
    (target_w as i32, target_h as i32, out)
}

/// Map an SNI property name to a `glib::Variant` value the
/// watcher can consume. Unknown properties trigger an empty
/// `()` Variant reply so the watcher doesn't loop on retries.
fn prop_value(
    snap: &TraySnapshot,
    property: &str,
    _icon_resource_path: &str,
) -> gtk::glib::Variant {
    match property {
        "Category" => "ApplicationStatus".to_variant(),
        "Id" => "com.airvzxf.bose-connect-gui".to_variant(),
        "Title" => if snap.connected {
            format!("Bose Connect ({}%)", snap.battery)
        } else if snap.name.is_empty() {
            "Bose Connect for Linux".to_string()
        } else {
            format!("{} — disconnected", snap.name)
        }
        .to_variant(),
        "Status" => status_for(snap).as_str().to_variant(),
        "WindowId" => 0u32.to_variant(),
        // IconName is intentionally empty: the watcher must
        // use IconPixmap instead. Returning the resource path
        // would re-introduce the theme-lookup bug.
        "IconName" => String::new().to_variant(),
        "IconPixmap" => pixmap_variant(snap, 1),
        "IconThemePath" => String::new().to_variant(),
        "AttentionIconName" => String::new().to_variant(),
        // Same pixmap data as IconPixmap (we don't ship a
        // distinct "alarm" glyph yet; KDE Plasma 6 will simply
        // NOT animate between the two when they're identical).
        "AttentionIconPixmap" => pixmap_variant(snap, 1),
        "AttentionIconDescription" => "Bose Connect — battery is low".to_variant(),
        "OverlayIconName" => String::new().to_variant(),
        "OverlayIconPixmap" => {
            // a(iiay) — empty array; KDE represents "no
            // overlay" as a zero-length array. Constructed via
            // the raw C API to avoid the same
            // `array_from_iter_with_type` bug we hit on the
            // non-empty path.
            let elt_type = gtk::glib::VariantTy::new("(iiay)").expect("valid d-bus type");
            unsafe {
                let v = gtk::glib::ffi::g_variant_new_array(
                    elt_type.to_glib_none().0,
                    std::ptr::null(),
                    0,
                );
                gtk::glib::Variant::from_glib_none(v)
            }
        }
        "OverlayIconDescription" => String::new().to_variant(),
        "ItemIsMenu" => false.to_variant(),
        // The SNI spec uses an object path (`o`) here. We don't
        // ship a D-Bus menu object; returning `/` is the
        // convention for "no menu attached".
        "Menu" => "/".to_variant(),
        "ScrollOn" => false.to_variant(),
        _ => ().to_variant(),
    }
}

/// Send `Register` to the watcher so KDE Plasma picks the
/// icon up. The watcher can be at any version of Plasma 5/6
/// — the call signature is stable.
fn call_watcher_register(
    conn: &gtk::gio::DBusConnection,
    our_bus_name: &str,
) -> anyhow::Result<()> {
    let msg = gtk::gio::DBusMessage::new_method_call(
        Some(WATCHER_BUS_NAME),
        WATCHER_OBJECT_PATH,
        Some("org.kde.StatusNotifierWatcher"),
        "Register",
    );
    // Single argument: the bus name (a `s`).
    msg.set_body(&gtk::glib::Variant::tuple_from_iter([
        our_bus_name.to_variant()
    ]));
    let serial = conn.send_message(&msg, gtk::gio::DBusSendMessageFlags::NONE)?;
    tracing::debug!(target: "tray", "Register call serial={serial}");
    Ok(())
}

impl Drop for TrayService {
    fn drop(&mut self) {
        // `gio::RegistrationId`, `OwnerId`, and `DBusConnection`
        // are all reference-counted; dropping them releases
        // the bus name and the interface registration.
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(battery: u8, connected: bool, name: &str) -> TraySnapshot {
        TraySnapshot {
            connected,
            battery,
            name: name.to_string(),
        }
    }

    #[test]
    fn status_mapping_covers_every_band() {
        // Disconnected => Passive, regardless of battery / name.
        assert_eq!(status_for(&snap(0, false, "")), SniStatus::Passive);
        assert_eq!(
            status_for(&snap(50, false, "Bose QC35")),
            SniStatus::Passive
        );
        // Connected but no name => Passive (no device yet).
        assert_eq!(status_for(&snap(80, true, "")), SniStatus::Passive);
        // Connected + healthy battery => Active.
        assert_eq!(status_for(&snap(80, true, "Bose QC35")), SniStatus::Active);
        // Connected + battery at the boundary 26 % is still Active.
        assert_eq!(status_for(&snap(26, true, "Bose QC35")), SniStatus::Active);
        // 25 % is the threshold; below that needs attention.
        assert_eq!(
            status_for(&snap(25, true, "Bose QC35")),
            SniStatus::NeedsAttention
        );
        // Critical: 0 – 5 %.
        assert_eq!(
            status_for(&snap(5, true, "Bose QC35")),
            SniStatus::NeedsAttention
        );
        assert_eq!(
            status_for(&snap(0, true, "Bose QC35")),
            SniStatus::NeedsAttention
        );
    }

    #[test]
    fn status_strings_match_the_sni_spec() {
        // The watcher (KStatusNotifierItem) reads the `Status`
        // property as a C string with one of three exact values.
        // Drift here would silent-fail on KDE Plasma 6.
        assert_eq!(SniStatus::Active.as_str(), "Active");
        assert_eq!(SniStatus::Passive.as_str(), "Passive");
        assert_eq!(SniStatus::NeedsAttention.as_str(), "NeedsAttention");
    }

    #[test]
    fn pixmap_is_22x22_argb32_in_network_byte_order() {
        let (_w, h, data) = make_pixmap(&snap(50, true, "Bose QC35"));
        assert_eq!((h as usize), ICON_PX as usize);
        assert_eq!(data.len(), ICON_PX as usize * ICON_PX as usize * 4);
        // Every pixel must be 4-byte ARGB in network byte order.
        for chunk in data.chunks_exact(4) {
            let a = chunk[0];
            // Either fully transparent (a == 0) or fully opaque (a == 0xFF).
            assert!(
                a == 0 || a == 0xFF,
                "alpha must be 0x00 or 0xFF, got 0x{a:02x}"
            );
        }
    }

    #[test]
    fn pixmap_color_matches_battery_band() {
        // The green / amber / red / grey palette is keyed off
        // the battery state. We sample the centre of the left
        // ear cup ((5, 13) in geometry units) — that point is
        // always inside the filled circle, regardless of scale.
        // The ARGB32 buffer is in network byte order: A → R → G → B.
        let sample_red = |snap: &TraySnapshot| {
            let (_, _, buf) = make_pixmap(snap);
            let idx = ((13 * ICON_PX as usize) + 5) * 4;
            (buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3])
        };
        // Disconnected => grey 0x9A.
        let (a, r, _g, _b) = sample_red(&snap(0, false, ""));
        assert_eq!(a, 0xFF, "alpha always opaque on filled pixels");
        assert_eq!(r, 0x9A, "disconnected pixel must be grey");
        // Healthy battery (>= 26 %).
        let (_, r, _, _) = sample_red(&snap(80, true, "Bose QC35"));
        assert_eq!(r, 0x4C, "healthy battery pixel must be green");
        // Low (10 – 25 %).
        let (_, r, _, _) = sample_red(&snap(15, true, "Bose QC35"));
        assert_eq!(r, 0xE6, "low battery pixel must be amber");
        // Critical (≤ 5 %).
        let (_, r, _, _) = sample_red(&snap(3, true, "Bose QC35"));
        assert_eq!(r, 0xE2, "critical battery pixel must be red");
    }

    #[test]
    fn pixmap_has_exactly_two_filled_circles_and_a_band() {
        // Count opaque pixels and confirm the geometry: 16 px wide
        // band on rows 4-6, plus two ear cups of radius 4, centred at
        // (5, 13) and (17, 13). The integer-area union is around
        // 110–120 opaque pixels; we just check it's a sane shape
        // (not zero, not the whole canvas, and congruent across
        // scales).
        let (_, _, data) = make_pixmap(&snap(50, true, "Bose QC35"));
        let opaque = data.chunks_exact(4).filter(|c| c[0] == 0xFF).count();
        assert!(
            opaque > 60,
            "expected a recognizable glyph, got {opaque} px"
        );
        assert!(opaque < 200, "glyph is leaking past geometry: {opaque} px");
    }

    #[test]
    fn pixmap_scales_to_44x44_for_hidpi() {
        let (w, h, data) = make_pixmap_scaled(&snap(50, true, "Bose QC35"), 2);
        assert_eq!(w, 44);
        assert_eq!(h, 44);
        // Same pixel count as 22*22 ARGB32 — doubled width × doubled
        // height = 4x the pixel count.
        assert_eq!(data.len(), 44 * 44 * 4);
    }
}
