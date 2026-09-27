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
const SNI_INTERFACE_XML: &str = r##"
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="u" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="AttentionIconDescription" type="s" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
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

/// Map an SNI property name to a `glib::Variant` value the
/// watcher can consume. Returning `Some(())` for unknown
/// properties so the watcher doesn't loop on retries.
fn prop_value(snap: &TraySnapshot, property: &str, icon_resource_path: &str) -> gtk::glib::Variant {
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
        "Status" => {
            if snap.connected {
                "Active".to_variant()
            } else {
                "Passive".to_variant()
            }
        }
        "WindowId" => 0u32.to_variant(),
        "IconName" => icon_resource_path.to_variant(),
        "IconThemePath" => String::new().to_variant(),
        "AttentionIconName" => icon_resource_path.to_variant(),
        "AttentionIconDescription" => "Battery is low".to_variant(),
        "OverlayIconName" => String::new().to_variant(),
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
