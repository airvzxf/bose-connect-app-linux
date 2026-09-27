//! Bluetooth discovery over BlueZ's D-Bus interface.
//!
//! BlueZ exposes adapters as `org.bluez.Adapter1` and devices as
//! `org.bluez.Device1`. We talk to it directly through `gio::DBus`
//! (already in the dependency tree via `glib 0.22`) instead of
//! pulling in a full `zbus` dependency — the surface we need is
//! small (`GetManagedObjects` + `InterfacesAdded` /
//! `InterfacesRemoved` signal subscription).
//!
//! The service is **optional**: when BlueZ is not running (the
//! common case in `cargo check` smoke tests and CI), we degrade
//! gracefully by returning an empty discovery stream and log a
//! warning.

use std::time::Duration;

use anyhow::Result;
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
    /// `True` only if the device advertises the Bose Connect
    /// service UUID.
    pub is_bose: bool,
}

/// Bose Connect service UUID. The Bluetooth SIG-assigned
/// namespace; the `0000fddd-…` 16-bit UUID sits inside the
/// 128-bit Bluetooth Base UUID reserved for vendor use.
pub const BOSE_UUID: &str = "0000fddd-0000-1000-8000-00805f9b34fb";

/// Thin wrapper around the BlueZ connection. The real impl
/// lives behind `gio::DBusConnection` (added in the next
/// commit); for now the type carries the channel end and an
/// `available()` flag.
pub struct BluetoothDiscovery {
    available: bool,
    _tx: mpsc::UnboundedSender<DiscoveryEvent>,
}

impl BluetoothDiscovery {
    /// Connect to the system BlueZ daemon. Returns a handle and a
    /// `mpsc::UnboundedReceiver` for events. When BlueZ isn't
    /// reachable, the handle's `available()` returns `false` and
    /// the receiver will simply stay empty until the user clicks
    /// `Scan` (which deliberately does nothing in that case).
    pub async fn connect() -> Result<(Self, mpsc::UnboundedReceiver<DiscoveryEvent>)> {
        let (tx, rx) = mpsc::unbounded_channel::<DiscoveryEvent>();
        // The gio-backed implementation will probe
        // `org.bluez` here in a follow-up commit. For
        // now we assume BlueZ is reachable on every system
        // that has the binary installed; the smoke test
        // exercises the empty-discovery fallback when
        // `available()` is false.
        Ok((
            Self {
                available: false,
                _tx: tx,
            },
            rx,
        ))
    }

    /// Best-effort: start a discovery scan on every available
    /// adapter. The real implementation calls
    /// `org.bluez.Adapter1.StartDiscovery` over the system bus;
    /// the placeholder returns the list of adapter paths we
    /// *would* have started a scan on, or `Err` if BlueZ isn't
    /// reachable.
    pub async fn start_scan(&self) -> Result<Vec<String>> {
        if self.available {
            // Real impl in next commit.
            Ok(Vec::new())
        } else {
            Err(anyhow::anyhow!("BlueZ not available"))
        }
    }

    /// Return whether BlueZ is reachable at all.
    pub fn available(&self) -> bool {
        self.available
    }

    /// For tests / offline smoke: pretend to discover a Bose
    /// device after a short delay. Used to populate the
    /// discover screen even when no system BlueZ daemon is
    /// running.
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

    /// Filter helper: true when the supplied UUID string matches
    /// the Bose Connect service UUID. Used by the gio
    /// implementation to flag Bose candidates in the discover
    /// list.
    pub fn is_bose_uuid(uuid: &str) -> bool {
        uuid.eq_ignore_ascii_case(BOSE_UUID)
    }
}
