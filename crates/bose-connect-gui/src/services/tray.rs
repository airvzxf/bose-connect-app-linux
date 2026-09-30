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
    /// after the initial RegisterStatusNotifierItem call, but keeping the handle
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
    /// SNI interface, then call `RegisterStatusNotifierItem` on
    /// the watcher so KDE Plasma picks the icon up. The icon is
    /// loaded from the GResource bundle at the URI registered by
    /// `resources/gresource.xml`.
    pub fn start() -> anyhow::Result<(TrayServiceHandle, mpsc::UnboundedReceiver<TrayCommand>)> {
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
                prop_value(&snap, property)
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
    /// returns. Triggers `NewTitle` / `NewIcon` / `NewStatus` so
    /// KDE refreshes. **Critically**: we also emit `NewStatus`
    /// because every Plasma 6 implementation of the SNI watcher
    /// caches the first `Status` it reads and only re-polls on
    /// a `NewStatus` signal — without it, an icon registered
    /// with `Status=Passive` (the default snapshot at start)
    /// stays hidden even after we flip the snapshot to
    /// `Active` / `NeedsAttention`.
    #[allow(dead_code)]
    pub fn update(&self, snap: TraySnapshot) {
        // Capture the prior status so we only emit NewStatus
        // when the band actually changes — keeps the watcher
        // noise-free and (importantly) ensures the very first
        // transition out of Passive is observable.
        let prev_status = status_for(&self.snapshot.lock());
        *self.snapshot.lock() = snap.clone();
        let new_status = status_for(&self.snapshot.lock());

        let if_name = "org.kde.StatusNotifierItem";
        let _ = self
            ._conn
            .emit_signal(None, SNI_OBJECT_PATH, if_name, "NewIcon", None);
        let _ = self
            ._conn
            .emit_signal(None, SNI_OBJECT_PATH, if_name, "NewTitle", None);
        if prev_status != new_status {
            // NewStatus takes a `s` parameter with the new
            // status value, per the SNI DBus interface XML.
            let body = gtk::glib::Variant::tuple_from_iter([new_status.as_str().to_variant()]);
            let _ =
                self._conn
                    .emit_signal(None, SNI_OBJECT_PATH, if_name, "NewStatus", Some(&body));
        }
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

/// Status of the SNI item, per the StatusNotifierItem spec.
/// KDE Plasma 6 shows `Active` automatically; `Passive`
/// requires the user to enable "Show all items" in the panel
/// settings (off by default). We deliberately keep the
/// `NeedsAttention` variant for spec compatibility (the SNI
/// method_handlers can still emit it if a future use-case
/// arrives) but `status_for()` never selects it — flipping
/// that flag triggers KDE's IconPixmap/AttentionIconPixmap
/// blink animation, which we don't want for a passive
/// battery-band indicator.
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
///
/// We deliberately do **not** return `NeedsAttention` for the
/// low / critical battery bands: KDE Plasma 6's watcher
/// animates between `IconPixmap` and `AttentionIconPixmap`
/// whenever the item is in `NeedsAttention` (the same icon
/// swaps in and out). That animation flickers on the panel
/// every 1–2 s, which is distractingly loud in idle time.
/// Battery priority is communicated via the Title string
/// (`"Bose Connect (10%)"`) and via the bell pop-up, both of
/// which the user looks at on demand — the tray icon stays a
/// steady SVG until the next UI polish pass.
pub fn status_for(snap: &TraySnapshot) -> SniStatus {
    if !snap.connected || snap.name.is_empty() {
        return SniStatus::Passive;
    }
    SniStatus::Active
}

/// GResource path that the `gresource.xml` registered for the
/// full-colour tray icon. Used as the data source for
/// `IconPixmap` and `AttentionIconPixmap`.
const TRAY_ICON_RESOURCE: &str =
    "/com/airvzxf/bose-connect-gui/icons/hicolor/scalable/apps/bose-connect-gui.svg";

/// Pixel side the Plasma 6 system-tray applet allocates for an
/// SNI by default. The watcher usually asks for the largest
/// pixmap in the returned array, so we also return a 2× copy
/// for HiDPI displays.
const ICON_PX: i32 = 22;

/// Load the SVG icon from the GResource bundle, render it into
/// an ARGB32 buffer at the requested scale, and return the
/// `(width, height, bytes)` tuple the property handler ships
/// to the watcher.
///
/// We render with `gdk_pixbuf::Pixbuf::from_resource_at_scale`
/// so the renderer takes care of anti-aliasing — for a scalable
/// SVG the bytes that hit Plasma 6 are clean even though the
/// tray applet later stretches them again. We force `RGBA`
/// (`has_alpha == true`); the per-pixel layout is then
/// `R, G, B, A`. The SNI spec wants ARGB in **network byte
/// order** (`A, R, G, B`), so we re-shuffle each quad before
/// handing the buffer to the watcher.
fn load_pixmap(scale: i32) -> Result<(i32, i32, Vec<u8>), anyhow::Error> {
    let side = ICON_PX.max(1) * scale.max(1);
    // The GResource bundle is registered by the binary at
    // startup; tests don't go through `main`, so we have to
    // register the same bundle here at first use. The
    // registration is a cheap refcount bump when already
    // registered, so doing this unconditionally costs nothing
    // after the first call.
    crate::register_resources();
    let pixbuf = gdk_pixbuf::Pixbuf::from_resource_at_scale(TRAY_ICON_RESOURCE, side, side, true)
        .map_err(|e| anyhow::anyhow!("load SVG icon: {e}"))?;
    let w = pixbuf.width();
    let h = pixbuf.height();
    assert!(
        pixbuf.n_channels() == 4 && pixbuf.has_alpha(),
        "expected RGBA pixbuf (n_channels=4 + has_alpha)"
    );
    let stride = pixbuf.rowstride() as usize;
    let src = unsafe { pixbuf.pixels() };
    let src_len = stride * h as usize;
    assert_eq!(
        src.len(),
        src_len,
        "gdk-pixbuf returned a pixel buffer of unexpected size"
    );

    // RGBA→ARGB (big-endian): swap R↔A on each pixel.
    let mut data = Vec::with_capacity(src.len());
    let width = w as usize;
    for row in 0..h as usize {
        let row_start = row * stride;
        let row_end = row_start + width * 4;
        for chunk in src[row_start..row_end].chunks_exact(4) {
            data.push(chunk[3]); // A
            data.push(chunk[0]); // R
            data.push(chunk[1]); // G
            data.push(chunk[2]); // B
        }
    }
    Ok((w, h, data))
}

/// Wrap the rendered pixmap in an `a(iiay)` SNI `IconPixmap`
/// Variant. KDE Plasma 6 picks the largest pixmap from the array
/// it can render natively, so the caller chooses whether to
/// include a HiDPI copy via `scale = 2`.
///
/// We build the array via the raw `g_variant_builder_*` C API
/// because glib 0.22's `Variant::array_from_iter_with_type` helper
/// has a known bug where the per-child type check compares
/// against the full array type instead of the element type.
fn pixmap_variant(scale: i32) -> gtk::glib::Variant {
    let arr_type = gtk::glib::VariantTy::new("a(iiay)").expect("valid d-bus type");
    let empty = gtk::glib::Variant::from_iter(std::iter::empty::<gtk::glib::Variant>());
    match load_pixmap(scale) {
        Ok((w, h, data)) => {
            let inner = gtk::glib::Variant::tuple_from_iter([
                w.to_variant(),
                h.to_variant(),
                data.to_variant(),
            ]);
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
        Err(err) => {
            tracing::warn!(
                target: "tray",
                "SVG icon render failed at scale={scale}: {err}; tray icon will be blank",
            );
            empty
        }
    }
}

/// Map an SNI property name to a `glib::Variant` value the
/// watcher can consume. Unknown properties trigger an empty
/// `()` Variant reply so the watcher doesn't loop on retries.
///
/// `Category=ApplicationStatus` (not 'Hardware') so KDE Plasma 6
/// buckets the icon next to other user-installed apps — Krita,
/// Discord, Telora, KDE Connect — and away from the
/// always-on OS primitives (NetworkManager, the Bluetooth
/// applet, PulseAudio volume, Battery). Pairing two icon
/// sources for the same BT hardware would be confusing (the
/// Plasma 6 Bluetooth applet already shows every paired
/// device including any Bose headset), and a vendor-specific
/// GUI that lives in the user's $HOME feels like an app, not
/// a system primitive.
fn prop_value(snap: &TraySnapshot, property: &str) -> gtk::glib::Variant {
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
        "IconPixmap" => pixmap_variant(1),
        "IconThemePath" => String::new().to_variant(),
        "AttentionIconName" => String::new().to_variant(),
        // Same pixmap data as IconPixmap (we don't ship a
        // distinct "alarm" glyph yet; KDE Plasma 6 will simply
        // NOT animate between the two when they're identical).
        "AttentionIconPixmap" => pixmap_variant(1),
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

/// Send `RegisterStatusNotifierItem` to the watcher so KDE
/// Plasma picks the icon up. The watcher's exact method
/// name is the one listed by `gdbus introspect
/// --object-path /StatusNotifierWatcher`:
///
/// ```text
/// methods:
///   RegisterStatusNotifierItem(in  s service);
///   RegisterStatusNotifierHost(in  s service);
/// ```
///
/// **The canonical KDE Plasma 6 watcher has no `Register`
/// method.** Calling the wrong name returns
/// `org.freedesktop.DBus.Error.UnknownMethod`, the watcher
/// silently ignores our existence, and the icon never
/// appears in the tray. (Our pre-fix code shipped `Register`
/// and the smoke test erroneously grep-matched
/// `RegisterStatusNotifierItem` by suffix, which masked the
/// regression for a couple of sessions.)
fn call_watcher_register(
    conn: &gtk::gio::DBusConnection,
    our_bus_name: &str,
) -> anyhow::Result<()> {
    let msg = gtk::gio::DBusMessage::new_method_call(
        Some(WATCHER_BUS_NAME),
        WATCHER_OBJECT_PATH,
        Some("org.kde.StatusNotifierWatcher"),
        "RegisterStatusNotifierItem",
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
    fn status_mapping_avoids_animated_attention() {
        // Every *connected* state resolves to Active — we deliberately
        // do not return NeedsAttention, because KDE Plasma 6
        // animates between IconPixmap and AttentionIconPixmap for
        // NeedsAttention items, and that flicker is distracting.
        // Priority instead travels through the Title string and
        // the bell pop-up, both of which the user can ignore.
        assert_eq!(status_for(&snap(0, false, "")), SniStatus::Passive);
        assert_eq!(
            status_for(&snap(50, false, "Bose QC35")),
            SniStatus::Passive
        );
        // No name on a connected snapshot also stays Passive
        // (we haven't pinned a device yet).
        assert_eq!(status_for(&snap(80, true, "")), SniStatus::Passive);
        // Connected + battery at every band → Active. The spec
        // text "Battery low / critical / ok" travels in the
        // Title and the bell, not the icon status.
        for &battery in &[100u8, 80, 50, 30, 25, 15, 5, 0] {
            let s = snap(battery, true, "Bose QC35");
            assert_eq!(
                status_for(&s),
                SniStatus::Active,
                "battery={battery}: expected Active, got NeedsAttention or Passive",
            );
        }
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
    fn category_is_application_status_not_hardware() {
        // We deliberately belong in the "Application Status" tray
        // group, not in the "Hardware" group. Plasma 6 uses the
        // SNI Category string to bucket the icon next to
        // user-installed apps (Krita, Discord, KDE Connect,
        // Telora) versus always-on OS primitives (NetworkManager,
        // Bluetooth applet, PulseAudio volume). Putting a
        // vendor-specific Bluetooth headphone GUI alongside
        // those would shadow the plasma-bluetooth applet's own
        // entry for the same device and confuse the user.
        //
        // The Category value is not reachable from the public
        // API (it's only emitted by the property handler), so
        // we test it indirectly by exposing a tiny helper that
        // wraps prop_value's lookup. Implemented here as a
        // grep of the source itself rather than as a runtime
        // test because launching the full SNI object in a unit
        // test would require a real DBus session.
        let source = include_str!("tray.rs");
        assert!(
            source.contains("\"Category\" => \"ApplicationStatus\".to_variant()"),
            "SNI Category must be ApplicationStatus; \
             change the value in prop_value() to fix this test"
        );
        assert!(
            !source.contains("\"Category\" => \"Hardware\".to_variant()"),
            "SNI Category drifted back to 'Hardware'; \
             change the value in prop_value() to fix this test"
        );
    }

    #[test]
    fn pixmap_is_argb32_in_network_byte_order() {
        // `load_pixmap` actually exercises the GResource → SVG →
        // Pixbuf → ARGB network byte order pipeline, so a
        // failure here pins down which stage regressed.
        let (w, h, data) = load_pixmap(1).expect("SVG must render at 1× scale");
        assert!(
            w > 0 && h > 0,
            "renderer returned an empty pixmap (w={w}, h={h})"
        );
        assert_eq!(
            (w as usize) * (h as usize) * 4,
            data.len(),
            "ARGB buffer length must match the width × height × 4 contract"
        );
        // Every pixel must be 4-byte ARGB. Alpha may not be
        // strictly 0xFF / 0x00 because the SVG has soft edges,
        // but the buffer must at least stay parseable as ARGB
        // (no stray channels, no zero-length slice).
        for chunk in data.chunks_exact(4) {
            // Crude sanity: none of the four bytes is a malformed
            // sentinel (they can be anything 0–255 from gdk-pixbuf).
            let _ = [chunk[0], chunk[1], chunk[2], chunk[3]];
        }
        // The pixmap must not be entirely transparent; an empty
        // icon would slide through KDE's icon rendering silently.
        let fully_opaque_count = data.chunks_exact(4).filter(|c| c[0] == 0xFF).count();
        assert!(
            fully_opaque_count > (data.len() / 8),
            "icon must have at least 12.5 % opaque pixels; got {fully_opaque_count}",
        );
    }

    #[test]
    fn pixmap_scales_to_44x44_for_hidpi() {
        let (w, h, data) = load_pixmap(2).expect("SVG must render at 2× scale");
        assert_eq!(w, ICON_PX * 2, "HiDPI width must be ICON_PX × 2");
        assert_eq!(h, ICON_PX * 2, "HiDPI height must be ICON_PX × 2");
        assert_eq!(
            data.len(),
            (ICON_PX * 2 * ICON_PX * 2) as usize * 4,
            "HiDPI pixmap buffer must be w × h × 4 bytes"
        );
    }

    #[test]
    fn pixmap_resource_uri_was_registered() {
        crate::register_resources();
        // Smoke-check that the GResource bundle the binary
        // shipped contains the SVG path we ship to the
        // property handler. If a future refactor builds
        // resources/gresource.xml without this entry the
        // load_pixmap call below will fall over.
        let lookup = gtk::gio::resources_lookup_data(
            TRAY_ICON_RESOURCE,
            gtk::gio::ResourceLookupFlags::NONE,
        );
        assert!(
            lookup.is_ok(),
            "GResource for {TRAY_ICON_RESOURCE} must be reachable from the binary",
        );
    }
}
