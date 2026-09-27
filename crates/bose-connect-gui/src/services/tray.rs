//! In-process placeholder for the KDE Plasma tray icon.
//!
//! The real `StatusNotifierItem` integration requires the `ksni`
//! crate and a running `plasma-workspace`'s StatusNotifierWatcher
//! to publish the registration. Both are available on the host
//! that ships this code (Arch Linux KDE Plasma 6) so a future
//! patch can move from the channel-only skeleton here to the
//! `ksni::TrayService` impl without touching the rest of the
//! GUI.
//!
//! For now this module is a thin command channel so the top-level
//! component can route "Show / Refresh / ToggleQuietMode" actions
//! back through the same reducer.

use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Default)]
pub struct TraySnapshot {
    pub connected: bool,
    pub battery: u8,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum TrayCommand {
    Show,
    Hide,
    ToggleQuietMode,
    Refresh,
    DisconnectAll,
    Quit,
}

pub struct TrayService {
    snapshot: Arc<Mutex<TraySnapshot>>,
    #[allow(dead_code)]
    tx: mpsc::UnboundedSender<TrayCommand>,
}

impl TrayService {
    pub async fn spawn(
        _dbus_name: &str,
    ) -> anyhow::Result<(TrayServiceHandle, mpsc::UnboundedReceiver<TrayCommand>)> {
        let (tx, rx) = mpsc::unbounded_channel::<TrayCommand>();
        let snapshot = Arc::new(Mutex::new(TraySnapshot::default()));
        // The real tray body would call `ksni::TrayService::new(...)`
        // and `service.spawn()` here. We keep that out until the
        // runtime dependency is reintroduced.
        Ok((TrayServiceHandle::new(snapshot.clone(), tx.clone()), rx))
    }

    /// Update the snapshot we want the next menu to render.
    pub fn update(&self, snap: TraySnapshot) {
        *self.snapshot.lock() = snap;
    }
}

pub struct TrayServiceHandle {
    snapshot: Arc<Mutex<TraySnapshot>>,
    tx: mpsc::UnboundedSender<TrayCommand>,
}

impl TrayServiceHandle {
    pub fn new(snapshot: Arc<Mutex<TraySnapshot>>, tx: mpsc::UnboundedSender<TrayCommand>) -> Self {
        Self { snapshot, tx }
    }

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

impl Drop for TrayService {
    fn drop(&mut self) {}
}
