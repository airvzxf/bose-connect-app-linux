//! Service layer for the GUI.

pub mod assets;
pub mod bluetooth;
pub mod device;
pub mod media_player;
pub mod notifications;
pub mod state;
pub mod tray;

pub use assets::{install_all as install_assets, DESKTOP_BASENAME, ICON_NAME};
pub use bluetooth::{BluetoothDiscovery, DiscoveryEvent, BOSE_UUID};
pub use device::{
    Capability, DeviceService, DeviceSnapshot, DynDeviceService, MockService, RealService,
};
pub use media_player::MediaPlayerService;
pub use notifications::{NotificationLevel, Notifications};
pub use state::{PersistedState, ProfileName, ProfileSettings, Profiles};
pub use tray::{TrayCommand, TrayService, TrayServiceHandle, TraySnapshot};
