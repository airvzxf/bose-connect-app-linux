//! Bluetooth discovery over BlueZ's D-Bus interface.
//!
//! BlueZ exposes adapters as `org.bluez.Adapter1` and devices as
//! `org.bluez.Device1`. We talk to it directly through `gio::DBus`
//! (already in the dependency tree via `glib 0.22`) instead of
//! pulling in a full `zbus` dependency. The surface we use is:
//!
//! - One-shot `GetManagedObjects` on `org.freedesktop.DBus.ObjectManager`
//!   to seed the discover list with already-known devices.
//! - Signal subscription on
//!   `org.freedesktop.DBus.ObjectManager.{InterfacesAdded,InterfacesRemoved}`
//!   for live updates as the kernel brings devices up / down.
//! - Per-device `StartDiscovery` on `org.bluez.Adapter1` to
//!   trigger an active scan when the user clicks "Scan".
//!
//! The service is **optional**: when BlueZ is not running (the
//! common case in `cargo check` smoke tests and CI), `connect()`
//! returns a handle whose `available()` is `false` and the GUI
//! continues without a discover list.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use gtk::glib::prelude::FromVariant;
use gtk::glib::VariantDict;
use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    AdapterPowered(bool),
    DeviceFound(DiscoveredDevice),
    DeviceRemoved(String), // D-Bus object path
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveredDevice {
    pub path: String,
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    /// True when BlueZ's `Modalias` names Bose as the vendor
    /// (`bluetooth:v009E…`), or the device lists a Bose UUID.
    pub is_bose: bool,
}

/// Bose Connect service UUID.
pub const BOSE_UUID: &str = "0000fddd-0000-1000-8000-00805f9b34fb";

/// Bluetooth SIG member UUID of Bose Corporation (`0xfebe`). Of the
/// QC Ultra, QC35 II and SoundLink Color II, only the QC35 II lists
/// it, and none lists [`BOSE_UUID`]; all three have a `Modalias` with
/// vendor `009E` (Bose), product = the device id, and the firmware
/// version (`bluetooth:v009Ep4066d0167` for a QC Ultra on 1.6.7).
pub const BOSE_COMPANY_UUID: &str = "0000febe-0000-1000-8000-00805f9b34fb";

/// True when a BlueZ `Modalias` (`bluetooth:vVVVVpPPPPdDDDD`) has
/// the Bose vendor id `009E`.
pub fn is_bose_modalias(modalias: &str) -> bool {
    bose_connect::audio_state::product_id_from_modalias(modalias).is_some()
}

/// BlueZ bus name and object root.
const BLUEZ_BUS_NAME: &str = "org.bluez";
const BLUEZ_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const OBJECT_MANAGER_OBJECT_PATH: &str = "/";
const BLUEZ_DEVICE_INTERFACE: &str = "org.bluez.Device1";
const BLUEZ_ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
const BLUEZ_TRANSPORT_INTERFACE: &str = "org.bluez.MediaTransport1";

/// Manager for `org.bluez` device discovery. Holds the system-bus
/// connection so the GUI can call into BlueZ from the glib main
/// thread, and forwards incoming `InterfacesAdded` /
/// `InterfacesRemoved` signals onto the mpsc the GUI consumes.
pub struct BluetoothDiscovery {
    conn: Option<gtk::gio::DBusConnection>,
    #[allow(dead_code)]
    subscriptions: Vec<gtk::gio::SignalSubscription>,
    tx: mpsc::UnboundedSender<DiscoveryEvent>,
    snapshot: Arc<Mutex<Vec<DiscoveredDevice>>>,
}

impl BluetoothDiscovery {
    /// Construct an empty discovery handle (no BlueZ
    /// connection). The mpsc receiver stays silent until
    /// `spawn_simulated_events` is called externally.
    pub fn empty() -> Self {
        let (tx, _rx) = mpsc::unbounded_channel::<DiscoveryEvent>();
        Self {
            conn: None,
            subscriptions: Vec::new(),
            tx,
            snapshot: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Connect to the system bus and start listening on the
    /// BlueZ ObjectManager. When BlueZ isn't running, this
    /// returns a handle with `available() == false`; the GUI
    /// keeps working but the discovery panel stays empty.
    pub async fn connect() -> Result<(Self, mpsc::UnboundedReceiver<DiscoveryEvent>)> {
        let (tx, rx) = mpsc::unbounded_channel::<DiscoveryEvent>();
        let conn =
            match gtk::gio::bus_get_sync(gtk::gio::BusType::System, None::<&gtk::gio::Cancellable>)
            {
                Ok(c) => c,
                Err(err) => {
                    tracing::warn!(
                        target: "bluez",
                        "system bus unavailable: {err}; running without discovery",
                    );
                    return Ok((
                        Self {
                            conn: None,
                            subscriptions: Vec::new(),
                            tx,
                            snapshot: Arc::new(Mutex::new(Vec::new())),
                        },
                        rx,
                    ));
                }
            };

        let mut discovery = Self {
            conn: Some(conn.clone()),
            subscriptions: Vec::new(),
            tx: tx.clone(),
            snapshot: Arc::new(Mutex::new(Vec::new())),
        };

        // Subscribe to InterfacesAdded / InterfacesRemoved on
        // the ObjectManager. The handler runs on the glib main
        // thread; we forward into the mpsc so the GUI consumer
        // doesn't need to be on that thread.
        let tx_added = tx.clone();
        let snap_added = Arc::clone(&discovery.snapshot);
        discovery.subscriptions.push(conn.subscribe_to_signal(
            Some(BLUEZ_BUS_NAME),
            Some(BLUEZ_OBJECT_MANAGER),
            Some("InterfacesAdded"),
            None,
            None,
            gtk::gio::DBusSignalFlags::NONE,
            move |signal| {
                handle_interfaces_added(signal.parameters, &tx_added, &snap_added);
            },
        ));
        let tx_removed = tx.clone();
        let snap_removed = Arc::clone(&discovery.snapshot);
        discovery.subscriptions.push(conn.subscribe_to_signal(
            Some(BLUEZ_BUS_NAME),
            Some(BLUEZ_OBJECT_MANAGER),
            Some("InterfacesRemoved"),
            None,
            None,
            gtk::gio::DBusSignalFlags::NONE,
            move |signal| {
                handle_interfaces_removed(signal.parameters, &tx_removed, &snap_removed);
            },
        ));

        // Seed the discover list with the current set of
        // devices so the user sees them even without an active
        // scan.
        if let Err(err) = discovery.seed_from_managed_objects() {
            tracing::warn!(target: "bluez", "seed: {err}");
        }

        Ok((discovery, rx))
    }

    /// Best-effort: start an active discovery scan on every
    /// available BlueZ adapter. The mock falls back to the
    /// pre-baked simulated discovery when BlueZ isn't running.
    pub async fn start_scan(&self) -> Result<Vec<String>> {
        let Some(conn) = &self.conn else {
            // Fallback for tests / CI: emit a single
            // simulated Bose device after a short delay.
            Self::spawn_simulated_events(self.tx.clone());
            return Ok(Vec::new());
        };

        // Refresh the adapter list (call GetManagedObjects) and
        // call StartDiscovery on every adapter we find.
        let adapter_paths = adapter_paths(conn)?;
        for path in &adapter_paths {
            let msg = gtk::gio::DBusMessage::new_method_call(
                Some(BLUEZ_BUS_NAME),
                path,
                Some(BLUEZ_ADAPTER_INTERFACE),
                "StartDiscovery",
            );
            match conn.send_message(&msg, gtk::gio::DBusSendMessageFlags::NONE) {
                Ok(serial) => tracing::debug!(
                    target: "bluez",
                    "StartDiscovery on {path} (serial={serial})",
                ),
                Err(err) => tracing::warn!(
                    target: "bluez",
                    "StartDiscovery on {path} failed: {err}",
                ),
            }
        }
        Ok(adapter_paths)
    }

    /// True when the system bus connection succeeded and BlueZ
    /// is reachable. When this is false, `start_scan` only
    /// emits the offline simulated event.
    pub fn available(&self) -> bool {
        self.conn.is_some()
    }

    /// Snapshot of known devices, useful for tests / GUI that
    /// needs the current list without consuming the mpsc.
    pub fn snapshot(&self) -> Vec<DiscoveredDevice> {
        self.snapshot.lock().clone()
    }

    /// For tests / offline smoke: pretend to discover a Bose
    /// device after a short delay.
    pub fn spawn_simulated_events(rx: mpsc::UnboundedSender<DiscoveryEvent>) {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let _ = rx.send(DiscoveryEvent::DeviceFound(DiscoveredDevice {
                path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
                address: "AA:BB:CC:DD:EE:FF".into(),
                name: "QuietCompanion".into(),
                paired: true,
                connected: false,
                is_bose: true,
            }));
        });
    }

    /// Filter helper: true when the supplied UUID string matches
    /// the Bose Connect service UUID.
    pub fn is_bose_uuid(uuid: &str) -> bool {
        uuid.eq_ignore_ascii_case(BOSE_UUID)
    }

    /// Run `GetManagedObjects` once at startup, walking the
    /// returned `a{oa{sa{sv}}}` to find every
    /// `org.bluez.Device1`.
    fn seed_from_managed_objects(&mut self) -> Result<()> {
        let Some(conn) = &self.conn else {
            return Ok(());
        };
        let body = conn.call_sync(
            Some(BLUEZ_BUS_NAME),
            OBJECT_MANAGER_OBJECT_PATH,
            BLUEZ_OBJECT_MANAGER,
            "GetManagedObjects",
            None,
            None,
            gtk::gio::DBusCallFlags::NONE,
            1_500,
            None::<&gtk::gio::Cancellable>,
        )?;
        walk_managed_objects(&body, &self.tx, &self.snapshot);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// signal + initial-state walkers
// ---------------------------------------------------------------------------

/// Decode `InterfacesAdded` body. The signature is
/// `(oa{sa{sv}})` — first the new object path, then a
/// dict-of-{interface → dict-of-{prop → variant}}.
fn handle_interfaces_added(
    body: &gtk::glib::Variant,
    tx: &mpsc::UnboundedSender<DiscoveryEvent>,
    snapshot: &Arc<Mutex<Vec<DiscoveredDevice>>>,
) {
    let path_v = body.try_child_value(0);
    let path: Option<String> = path_v.as_ref().and_then(|v| v.str().map(str::to_owned));
    let Some(path) = path else { return };

    let iface_v = match body.try_child_value(1) {
        Some(v) => v,
        None => return,
    };

    // `iface_v` is the `a{sa{sv}}` dict of
    // interface-name → props; only the `a{sv}` props fit a
    // `VariantDict`.
    let Some(dev_props_v) = interface_props(&iface_v, BLUEZ_DEVICE_INTERFACE) else {
        return;
    };

    let dev_dict = VariantDict::new(Some(&dev_props_v));
    let Some(dev) = decode_device(&path, &dev_dict) else {
        return;
    };

    let mut snap = snapshot.lock();
    if let Some(slot) = snap.iter_mut().find(|d| d.path == dev.path) {
        *slot = dev.clone();
    } else {
        snap.push(dev.clone());
    }
    let _ = tx.send(DiscoveryEvent::DeviceFound(dev));
}

/// Decode `InterfacesRemoved` body. The signature is
/// `(oas)` — the object path and a list of interface names.
fn handle_interfaces_removed(
    body: &gtk::glib::Variant,
    tx: &mpsc::UnboundedSender<DiscoveryEvent>,
    snapshot: &Arc<Mutex<Vec<DiscoveredDevice>>>,
) {
    let path_v = body.try_child_value(0);
    let path: Option<String> = path_v.as_ref().and_then(|v| v.str().map(str::to_owned));
    let Some(path) = path else { return };

    let mut snap = snapshot.lock();
    let before = snap.len();
    snap.retain(|d| d.path != path);
    if snap.len() != before {
        let _ = tx.send(DiscoveryEvent::DeviceRemoved(path));
    }
}

/// Walk the body of `GetManagedObjects` (`a{oa{sa{sv}}}`)
/// and emit one `DeviceFound` event for every
/// `org.bluez.Device1` object that has the `Address`
/// property set.
fn walk_managed_objects(
    body: &gtk::glib::Variant,
    tx: &mpsc::UnboundedSender<DiscoveryEvent>,
    snapshot: &Arc<Mutex<Vec<DiscoveredDevice>>>,
) {
    // The reply is a tuple holding the `a{oa{sa{sv}}}` dict.
    let body = &body.try_child_value(0).unwrap_or_else(|| body.clone());
    let n = body.n_children();
    for i in 0..n {
        let entry = match body.try_child_value(i) {
            Some(v) => v,
            None => continue,
        };
        // Each element of the outer array is a dict-entry
        // `{oa{sa{sv}}}`; the key is the object path and the
        // value is the `a{sa{sv}}` interface dict.
        let path_v = entry.try_child_value(0);
        let path: Option<String> = path_v.as_ref().and_then(|v| v.str().map(str::to_owned));
        let Some(path) = path else { continue };
        let iface_v = match entry.try_child_value(1) {
            Some(v) => v,
            None => continue,
        };
        let Some(dev_props_v) = interface_props(&iface_v, BLUEZ_DEVICE_INTERFACE) else {
            continue;
        };
        let dev_dict = VariantDict::new(Some(&dev_props_v));
        let Some(dev) = decode_device(&path, &dev_dict) else {
            continue;
        };
        let mut snap = snapshot.lock();
        if let Some(slot) = snap.iter_mut().find(|d| d.path == dev.path) {
            *slot = dev.clone();
        } else {
            snap.push(dev.clone());
        }
        let _ = tx.send(DiscoveryEvent::DeviceFound(dev));
    }
}

/// Extract a `DiscoveredDevice` from an `org.bluez.Device1`
/// props dict.
fn decode_device(path: &str, device_dict: &VariantDict) -> Option<DiscoveredDevice> {
    // `VariantDict::lookup::<T>("key")` returns
    // `Result<Option<T>, ...>`. We collapse both errors to
    // `None` with `.ok().and_then(|inner| inner)`.
    fn dict_lookup<T>(dict: &VariantDict, key: &str) -> Option<T>
    where
        T: FromVariant,
    {
        match dict.lookup::<T>(key) {
            Ok(Some(v)) => Some(v),
            _ => None,
        }
    }
    let address: String = dict_lookup(device_dict, "Address")?;
    let name: String = dict_lookup(device_dict, "Name").unwrap_or_default();
    let paired: bool = dict_lookup(device_dict, "Paired").unwrap_or(false);
    let connected: bool = dict_lookup(device_dict, "Connected").unwrap_or(false);
    let uuids: Vec<String> = dict_lookup(device_dict, "UUIDs").unwrap_or_default();
    let modalias: String = dict_lookup(device_dict, "Modalias").unwrap_or_default();
    let is_bose = is_bose_modalias(&modalias)
        || uuids.iter().any(|u| {
            BOSE_UUID.eq_ignore_ascii_case(u) || BOSE_COMPANY_UUID.eq_ignore_ascii_case(u)
        });

    Some(DiscoveredDevice {
        path: path.to_string(),
        address,
        name,
        paired,
        connected,
        is_bose,
    })
}

/// Walk the body of `GetManagedObjects` and return the
/// adapter (`/org/bluez/hci*`) paths.
fn adapter_paths(conn: &gtk::gio::DBusConnection) -> Result<Vec<String>> {
    let body = conn.call_sync(
        Some(BLUEZ_BUS_NAME),
        OBJECT_MANAGER_OBJECT_PATH,
        BLUEZ_OBJECT_MANAGER,
        "GetManagedObjects",
        None,
        None,
        gtk::gio::DBusCallFlags::NONE,
        1_500,
        None::<&gtk::gio::Cancellable>,
    )?;
    let body = body.try_child_value(0).unwrap_or(body);
    let n = body.n_children();
    let mut out = Vec::new();
    for i in 0..n {
        let entry = match body.try_child_value(i) {
            Some(v) => v,
            None => continue,
        };
        let path_v = entry.try_child_value(0);
        let path: Option<String> = path_v.as_ref().and_then(|v| v.str().map(str::to_owned));
        let Some(path) = path else { continue };
        if !path.contains("/hci") || path.contains("/dev_") {
            continue;
        }
        let iface_v = match entry.try_child_value(1) {
            Some(v) => v,
            None => continue,
        };
        if interface_props(&iface_v, BLUEZ_ADAPTER_INTERFACE).is_some() {
            out.push(path);
        }
    }
    Ok(out)
}

/// How BlueZ sees the audio link to the device at `address`: its
/// `org.bluez.Device1` and the `org.bluez.MediaTransport1` objects
/// that point at it. Blocking (one `GetManagedObjects`, 1.5 s
/// timeout); safe to call off the main thread. `None` when the
/// system bus or BlueZ is not reachable.
pub fn audio_state(address: &str) -> Option<bose_connect::AudioState> {
    let conn =
        gtk::gio::bus_get_sync(gtk::gio::BusType::System, None::<&gtk::gio::Cancellable>).ok()?;
    let body = conn
        .call_sync(
            Some(BLUEZ_BUS_NAME),
            OBJECT_MANAGER_OBJECT_PATH,
            BLUEZ_OBJECT_MANAGER,
            "GetManagedObjects",
            None,
            None,
            gtk::gio::DBusCallFlags::NONE,
            1_500,
            None::<&gtk::gio::Cancellable>,
        )
        .ok()?;
    // The reply is a tuple holding the `a{oa{sa{sv}}}` dict.
    let objects = body.try_child_value(0).unwrap_or(body);
    let mut entries = Vec::new();
    for i in 0..objects.n_children() {
        let Some(entry) = objects.try_child_value(i) else {
            continue;
        };
        let path = entry
            .try_child_value(0)
            .and_then(|v| v.str().map(str::to_owned));
        let Some(path) = path else { continue };
        let Some(ifaces) = entry.try_child_value(1) else {
            continue;
        };
        // `ifaces` is `a{sa{sv}}`; only its `a{sv}` values fit a
        // `VariantDict`.
        entries.push((path, ifaces));
    }
    let device = entries.iter().find_map(|(path, ifaces)| {
        let props = VariantDict::new(Some(&interface_props(ifaces, BLUEZ_DEVICE_INTERFACE)?));
        let addr: String = props.lookup("Address").ok().flatten()?;
        addr.eq_ignore_ascii_case(address).then(|| {
            let connected: bool = props.lookup("Connected").ok().flatten().unwrap_or(false);
            let product = props
                .lookup::<String>("Modalias")
                .ok()
                .flatten()
                .and_then(|m| bose_connect::audio_state::product_id_from_modalias(&m));
            (path.clone(), connected, product)
        })
    });
    let Some((device_path, connected, product)) = device else {
        return Some(bose_connect::AudioState::classify::<&str, &str>(
            None,
            None,
            &[],
        ));
    };
    let transports: Vec<(String, String)> = entries
        .iter()
        .filter_map(|(path, ifaces)| {
            let props =
                VariantDict::new(Some(&interface_props(ifaces, BLUEZ_TRANSPORT_INTERFACE)?));
            let device = props.lookup_value("Device", None)?;
            if device.str() != Some(device_path.as_str()) {
                return None;
            }
            let state = props
                .lookup::<String>("State")
                .ok()
                .flatten()
                .unwrap_or_else(|| "idle".to_owned());
            Some((path.clone(), state))
        })
        .collect();
    Some(bose_connect::AudioState::classify(
        Some(connected),
        product,
        &transports,
    ))
}

/// The `a{sv}` properties of `interface` in an `a{sa{sv}}`
/// interface dict.
fn interface_props(ifaces: &gtk::glib::Variant, interface: &str) -> Option<gtk::glib::Variant> {
    (0..ifaces.n_children()).find_map(|i| {
        let entry = ifaces.try_child_value(i)?;
        (entry.try_child_value(0)?.str()? == interface)
            .then(|| entry.try_child_value(1))
            .flatten()
    })
}

/// Convenience map keyed by path — used by tests that walk
/// the discovery state.
#[allow(dead_code)]
pub fn indexed_snapshot(devices: &[DiscoveredDevice]) -> HashMap<String, DiscoveredDevice> {
    devices
        .iter()
        .map(|d| (d.path.clone(), d.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::is_bose_modalias;

    #[test]
    fn bose_modalias() {
        assert!(is_bose_modalias("bluetooth:v009Ep4066d0167"));
        assert!(is_bose_modalias("bluetooth:v009ep400Dd0401"));
        assert!(!is_bose_modalias("usb:v05ACp0257d0001"));
        assert!(!is_bose_modalias(""));
    }
}
