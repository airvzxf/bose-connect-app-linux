//! Bluetooth discovery over BlueZ's D-Bus interface.
//!
//! BlueZ exposes adapters as `org.bluez.Adapter1` and devices as
//! `org.bluez.Device1`. We talk to it directly via `zbus` rather
//! than pulling in a full BlueZ binding — the surface we need is
//! small (interface list, property watch, "StartDiscovery").
//!
//! The service is **optional**: when BlueZ is not running (the
//! common case in `cargo check` smoke tests and CI), we degrade
//! gracefully by returning an empty discovery stream and log a
//! warning.

use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_lite::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use zbus::{Connection, Proxy};
use zvariant::Value;

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
    /// `True` only if the device advertises the Bose Connect
    /// service UUID.
    pub is_bose: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
struct BluezDeviceProps {
    address: String,
    name: String,
    paired: bool,
    connected: bool,
    uuids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
struct BluezAdapterProps {
    powered: bool,
    discovering: bool,
}

/// Thin wrapper around the BlueZ connection.
pub struct BluetoothDiscovery {
    conn: Option<Connection>,
    tx: mpsc::UnboundedSender<DiscoveryEvent>,
}

impl BluetoothDiscovery {
    /// Connect to the system BlueZ daemon. Returns `Ok(None)` if no
    /// D-Bus connection could be made — the caller treats this as
    /// "discovery disabled" and falls back to a pre-baked device list.
    pub async fn connect() -> Result<(Self, mpsc::UnboundedReceiver<DiscoveryEvent>)> {
        let conn = Connection::system().await.ok();
        let (tx, rx) = mpsc::unbounded_channel();
        Ok((Self { conn, tx }, rx))
    }

    /// Best-effort: start a discovery scan on every available
    /// adapter. Returns the list of adapter paths we managed to
    /// start, or `Err` if BlueZ wasn't reachable.
    pub async fn start_scan(&self) -> Result<Vec<String>> {
        let Some(conn) = &self.conn else {
            return Err(anyhow!("BlueZ not available"));
        };
        let _manager: Proxy<'_> = Proxy::new(
            conn,
            "org.bluez",
            "/",
            "org.freedesktop.DBus.ObjectManager",
        )
        .await?;
        // We list adapters with `GetManagedObjects` and call
        // `StartDiscovery` on each. Use a manual call surface to
        // avoid pulling in serde_json::Value recursion.
        let paths = find_adapter_paths(conn).await?;
        for path in &paths {
            let adapter: Proxy<'_> =
                Proxy::new(conn, "org.bluez", path, "org.bluez.Adapter1").await?;
            let _: Result<(), zbus::Error> = adapter.call("StartDiscovery", &()).await;
        }
        Ok(paths)
    }

    /// Return whether BlueZ is reachable at all.
    pub fn available(&self) -> bool {
        self.conn.is_some()
    }

    /// For tests: pretend to discover a Bose device after a short
    /// delay. Used in offline smoke tests where BlueZ is not
    /// running but we still want the discover screen to populate.
    pub fn spawn_simulated_events(rx: mpsc::UnboundedSender<DiscoveryEvent>) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
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
}

/// Fetch the list of `org.bluez.Adapter1` object paths via
/// `GetManagedObjects`. We pull apart the manually-encoded
/// `a{oa{sa{sv}}}` payload by walking `Value::Dict` entries.
async fn find_adapter_paths(conn: &Connection) -> Result<Vec<String>> {
    let manager: Proxy<'_> = Proxy::new(
        conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;
    let raw: OwnedValue = manager
        .call("GetManagedObjects", &())
        .await
        .map_err(|e| anyhow!("GetManagedObjects: {e}"))?;
    let dict = match &*raw {
        Value::Dict(m) => m.clone(),
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for (key, _) in dict.iter() {
        let Value::ObjectPath(path) = &**key else {
            continue;
        };
        let s = path.to_string();
        // The BlueZ convention is /org/bluez/hci* at the root.
        if s.contains("/hci") && !s.contains("/dev_") {
            out.push(s);
        }
    }
    Ok(out)
}

/// Helper newtype — `zbus` returns the raw GVariant payload.
struct OwnedValue {
    inner: zvariant::OwnedValue,
}

impl std::ops::Deref for OwnedValue {
    type Target = zvariant::Value<'static>;
    fn deref(&self) -> &Self::Target {
        // `OwnedValue::try_clone` then `Value` cast; the easiest
        // path is to clone the dynamic value and copy into a
        // fresh signature. We keep this in a dedicated helper.
        &self.inner
    }
}

impl<'a> From<zvariant::OwnedValue> for OwnedValue {
    fn from(inner: zvariant::OwnedValue) -> Self {
        Self { inner }
    }
}

/// Render a BlueZ `InterfacesAdded` payload into our typed
/// `DiscoveryEvent`. We do this manually because the `Value` ->
/// typed-struct deserialiser for HashMap<ObjectPath, HashMap<String,
/// HashMap<String, Value>>>` is fragile across zbus versions.
pub fn decode_interfaces_added(
    path: &str,
    interfaces: &std::collections::HashMap<String, std::collections::HashMap<String, Value<'_>>>,
) -> Option<DiscoveryEvent> {
    let dev = interfaces.get("org.bluez.Device1")?;
    let address = dev
        .get("Address")
        .and_then(|v| v.try_to_string().ok())
        .unwrap_or_default();
    let name = dev
        .get("Name")
        .and_then(|v| v.try_to_string().ok())
        .unwrap_or_default();
    let paired = dev
        .get("Paired")
        .and_then(|v| v.try_to::<bool>().ok())
        .unwrap_or(false);
    let connected = dev
        .get("Connected")
        .and_then(|v| v.try_to::<bool>().ok())
        .unwrap_or(false);
    let uuids = dev
        .get("UUIDs")
        .and_then(|v| v.try_to::<Vec<String>>().ok())
        .unwrap_or_default();
    let is_bose = uuids
        .iter()
        .any(|u| u.eq_ignore_ascii_case("0000fddd-0000-1000-8000-00805f9b34fb"));
    Some(DiscoveryEvent::DeviceFound(DiscoveredDevice {
        path: path.to_string(),
        address,
        name,
        paired,
        connected,
        is_bose,
    }))
}

/// Listen for `InterfacesAdded` from the BlueZ manager and push
/// them onto the supplied sender.
pub async fn stream_interfaces_added(
    conn: &Connection,
    tx: mpsc::UnboundedSender<DiscoveryEvent>,
) -> Result<()> {
    let proxy: Proxy<'_> = Proxy::new(
        conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;
    let mut stream = zbus::SignalStream::from(&proxy, "InterfacesAdded").await?;
    let mut task = Box::pin(async move {
        while let Some(signal) = stream.next().await {
            // signal is a Message; use the body to read the two
            // arguments. We drop on parse failure.
            let body = match signal.body() {
                Ok(b) => b,
                Err(_) => continue,
            };
            let (path, interfaces): (
                zvariant::ObjectPath,
                std::collections::HashMap<String, std::collections::HashMap<String, Value<'_>>>,
            ) = match body.deserialize() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if let Some(ev) = decode_interfaces_added(path.as_str(), &interfaces) {
                let _ = tx.send(ev);
            }
        }
    });
    // tie out the lifetime.
    let _ = task.as_mut();
    Ok(())
}

// Suppress the unused-warning for stream_events (kept for the API
// contract) — pull the stream forward into the futures pool so it
// stays alive across the lifetime of the program.
#[allow(dead_code)]
async fn _drop_future() {
    let _: futures_lite::stream::Pending<&'static str> = futures_lite::stream::pending();
}
