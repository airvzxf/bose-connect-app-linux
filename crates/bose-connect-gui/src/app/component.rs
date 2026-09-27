//! The Relm4 component for the top-level window.
//!
//! Simple, direct, one-model one-view. The model's `service` runs
//! on the same thread; the GUI's interactions call into it
//! synchronously. This is acceptable for the first iteration
//! because the `bose_connect` library itself blocks ~1s per
//! command — the GTK main loop is the wrong place to call it
//! twice in a row, so we wrap each call in `relm4::spawn`
//! which uses an `tokio` runtime under the hood.

use std::sync::Arc;
use std::time::Duration;

use adw;
use gtk::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};

use crate::app::model::{AppModel, AppMsg, ConnectionState, TICK_INTERVAL_MS};
use crate::services::device::DeviceService;
use crate::services::device::DeviceSnapshot;

pub struct AppComponent {
    pub model: AppModel,
    /// Cached `adw::Application` recovered from the root window.
    pub app: adw::Application,
}

#[derive(Debug)]
pub struct AppWidgets;

impl SimpleComponent for AppComponent {
    type Init = (Arc<dyn DeviceService>, adw::Application);
    type Input = AppMsg;
    type Output = ();
    type Root = adw::ApplicationWindow;
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        // Built lazily by the binary's adw::Application and then
        // handed back through `Init`. Falling back to a bare root
        // here is *only* for code paths that haven't been wired
        // through `ComponentBuilder::launch_with_props`.
        let app = relm4::main_application();
        adw::ApplicationWindow::new(&app)
    }

    fn init(
        (service, app): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Best-effort auto-connect: only when the persisted file
        // already has a saved address. The binary would normally
        // skip this when running with `--mock --address`.
        let model = AppModel::new(service);
        if model.persisted.last_address.is_some() {
            sender.input(AppMsg::InitialConnect);
        } else {
            sender.input(AppMsg::SetConnection(ConnectionState::NotStarted));
        }

        // Driving tick — gives Relm4 a re-render cadence.
        let tick_input = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(TICK_INTERVAL_MS));
            loop {
                interval.tick().await;
                tick_input.emit(AppMsg::BatteryTick(0));
            }
        });

        // Build the widget tree once and graft it onto the root.
        let root_widget = crate::app::view::build_root(&app, &model, sender.input_sender());
        root.set_child(Some(&root_widget));

        ComponentParts {
            model: AppComponent { model, app },
            widgets: AppWidgets,
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::InitialConnect => {
                if let Some(addr) = self.model.persisted.last_address.clone() {
                    let _ = self.model.service.connect(&addr).map(|snap| {
                        self.on_snapshot(snap);
                    });
                } else {
                    self.model
                        .log_info("no previous address; opening discovery scan");
                    self.model.connection = ConnectionState::Discovering;
                }
            }
            AppMsg::Connect(addr) => {
                self.model.connection = ConnectionState::Connecting(addr.clone());
                self.model.log_info(format!("connecting to {addr}"));
                match self.model.service.connect(&addr) {
                    Ok(snap) => self.on_snapshot(snap),
                    Err(e) => {
                        self.model.connection = ConnectionState::Error(e.to_string());
                        self.model.log_error(format!("connect failed: {e}"));
                    }
                }
            }
            AppMsg::SnapshotReady(snap) => self.on_snapshot(snap),
            AppMsg::SnapshotError(err) => {
                self.model.connection = ConnectionState::Error(err.clone());
                self.model.log_error(err);
            }
            AppMsg::Refresh => match self.model.service.refresh() {
                Ok(snap) => self.on_snapshot(snap),
                Err(e) => self.model.log_error(format!("refresh failed: {e}")),
            },
            AppMsg::Refreshed(snap) => self.on_snapshot(snap),
            AppMsg::RefreshFailed(err) => self.model.log_error(format!("refresh failed: {err}")),

            AppMsg::SetNoiseCancelling(level) => {
                let r = self.model.service.set_noise_cancelling(level);
                self.dispatch_result(r, "set_noise_cancelling");
            }
            AppMsg::SelfVoice(level) | AppMsg::SetSelfVoice(level) => {
                let r = self.model.service.set_self_voice(level);
                self.dispatch_result(r, "set_self_voice");
            }
            AppMsg::SetAutoOff(minutes) => {
                let r = self.model.service.set_auto_off(minutes);
                self.dispatch_result(r, "set_auto_off");
            }
            AppMsg::SetLanguage {
                language,
                voice_prompts,
            } => {
                let r = self.model.service.set_language(language, voice_prompts);
                self.dispatch_result(r, "set_language");
            }
            AppMsg::SetVoicePrompts(on) => {
                let lang = self
                    .model
                    .snapshot
                    .as_ref()
                    .map(|s| {
                        bose_connect::PromptLanguage::from_u8(
                            s.status.language & bose_connect::VP_MASK,
                        )
                        .unwrap_or(bose_connect::PromptLanguage::En)
                    })
                    .unwrap_or(bose_connect::PromptLanguage::En);
                let r = self.model.service.set_language(lang, on);
                self.dispatch_result(r, "set_voice_prompts");
            }
            AppMsg::SetPairing(on) => {
                let r = self.model.service.set_pairing(on);
                self.dispatch_result(r, "set_pairing");
            }
            AppMsg::SetName(_name) => self.model.log_info("name change requested"),
            AppMsg::ApplyProfile(profile) => {
                let p = crate::services::state::Profiles::resolve(profile);
                self.model
                    .log_info(format!("applying profile {}", profile.label()));
                if let Some(nc) = p.noise_cancelling {
                    self.dispatch_result(self.model.service.set_noise_cancelling(nc), "nc");
                }
                if let Some(voice) = p.voice_prompts {
                    let lang = bose_connect::PromptLanguage::En;
                    self.dispatch_result(self.model.service.set_language(lang, voice), "voice");
                }
                if let Some(auto) = p.auto_off {
                    self.dispatch_result(self.model.service.set_auto_off(auto), "auto_off");
                }
                self.model.persisted.active_profile = Some(profile);
                self.model.persisted.save();
            }
            AppMsg::ToggleQuietMode => {
                self.model.quiet_mode = !self.model.quiet_mode;
                self.model.log_info(format!(
                    "quiet mode {}",
                    if self.model.quiet_mode { "on" } else { "off" }
                ));
                if self.model.quiet_mode {
                    self.dispatch_result(
                        self.model
                            .service
                            .set_noise_cancelling(bose_connect::NoiseCancelling::High),
                        "quiet_nc",
                    );
                    self.dispatch_result(
                        self.model
                            .service
                            .set_language(bose_connect::PromptLanguage::En, false),
                        "quiet_voice",
                    );
                    self.dispatch_result(
                        self.model
                            .service
                            .set_auto_off(bose_connect::AutoOff::Never),
                        "quiet_auto_off",
                    );
                }
            }
            AppMsg::PairedConnect(addr) => {
                let r = self.model.service.connect_device(addr);
                self.dispatch_result(r, "connect_device");
            }
            AppMsg::PairedDisconnect(addr) => {
                let r = self.model.service.disconnect_device(addr);
                self.dispatch_result(r, "disconnect_device");
            }
            AppMsg::PairedRemove(addr) => {
                let r = self.model.service.remove_device(addr);
                self.dispatch_result(r, "remove_device");
            }
            AppMsg::TrayEvent(cmd) => match cmd {
                crate::services::tray::TrayCommand::Show => self.model.log_info("tray: show"),
                crate::services::tray::TrayCommand::Hide => self.model.log_info("tray: hide"),
                crate::services::tray::TrayCommand::Refresh => match self.model.service.refresh() {
                    Ok(snap) => self.on_snapshot(snap),
                    Err(e) => self.model.log_error(format!("tray refresh failed: {e}")),
                },
                crate::services::tray::TrayCommand::ToggleQuietMode => {
                    self.model.quiet_mode = !self.model.quiet_mode;
                    self.model.log_info("toggle quiet mode (tray)");
                }
                crate::services::tray::TrayCommand::DisconnectAll => {
                    self.model.log_info("tray: disconnect all")
                }
                crate::services::tray::TrayCommand::Quit => relm4::main_application().quit(),
            },
            AppMsg::SetConnection(state) => {
                self.model.connection = state;
            }
            AppMsg::BatteryTick(_) => {
                // Trigger periodic re-renders. The mock ticks drain
                // the battery in `transport.rs`; we mirror whatever
                // is in the snapshot into the sparkline history.
                if let Some(snap) = &self.model.snapshot {
                    let battery = snap.battery;
                    self.model.history.push(battery);
                    if self.model.history.len() > 32 {
                        self.model.history.remove(0);
                    }
                }
            }
            AppMsg::DiscoveryFound { address, name } => {
                self.model
                    .log_info(format!("discovered: {name} ({address})"));
                self.model.discovery.push((address, name));
            }
            AppMsg::Acknowledge => self.model.log.clear(),
        }
    }
}

impl AppComponent {
    fn on_snapshot(&mut self, snap: DeviceSnapshot) {
        self.model.snapshot = Some(snap.clone());
        self.model.connection = ConnectionState::Connected;
        self.model.history = vec![snap.battery];
        self.model
            .log_success(format!("connected to {} ({}%)", snap.name, snap.battery));
        self.model.persisted.last_address = Some(crate::i18n::format_address(snap.address));
        self.model.persisted.save();
    }

    fn dispatch_result(&mut self, r: anyhow::Result<DeviceSnapshot>, label: &str) {
        match r {
            Ok(snap) => {
                self.model.log_success(format!("{label}: ok"));
                self.on_snapshot(snap);
            }
            Err(e) => self.model.log_error(format!("{label}: {e}")),
        }
    }
}
