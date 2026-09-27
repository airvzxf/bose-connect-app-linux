//! Widget sub-trees — pure functions that take `&AppModel` and a
//! message-sending handle and return a fully-formed widget.
//!
//! We keep them in a single `widgets.rs` rather than a folder
//! because they share a lot of helpers (the `tile` factory, the
//! segmented control builder, etc.) and the file is still small
//! enough to navigate with the keyboard.

use adw;
use gtk::prelude::*;
use gtk::{glib, Box as GtkBox, Button, Label, ListBox, ListBoxRow, Orientation, ToggleButton};

use crate::app::model::{AppModel, AppMsg, ConnectionState, LogLevel};
use crate::i18n;

/// Convenience extension so widget code stays readable. The
/// GUI is a single producer (`flume::Sender`) on top of
/// `glib::MainContext::spawn_local`, so any error here means the
/// pump is gone — the GUI is tearing down. Swallowing the error
/// keeps the closure bodies one-liners.
pub trait SenderExt {
    fn send_app(&self, msg: AppMsg);
}

impl SenderExt for flume::Sender<AppMsg> {
    fn send_app(&self, msg: AppMsg) {
        let _ = self.send(msg);
    }
}

// ---------------------------------------------------------------------------
// Shared factories
// ---------------------------------------------------------------------------

fn tile(class: &str) -> GtkBox {
    let b = GtkBox::new(Orientation::Vertical, 8);
    b.add_css_class("tile");
    b.add_css_class(class);
    b.set_hexpand(true);
    b.set_vexpand(true);
    b
}

fn tile_header(title: &str, subtitle: &str) -> GtkBox {
    let h = GtkBox::new(Orientation::Horizontal, 8);
    h.add_css_class("tile-header");

    let column = GtkBox::new(Orientation::Vertical, 2);
    column.set_hexpand(true);
    let title_label = Label::new(Some(title));
    title_label.add_css_class("tile-title");
    title_label.set_xalign(0.0);
    let subtitle_label = Label::new(Some(subtitle));
    subtitle_label.add_css_class("tile-subtitle");
    subtitle_label.set_xalign(0.0);
    column.append(&title_label);
    column.append(&subtitle_label);
    h.append(&column);
    h
}

fn segmented<F>(options: &[(&str, &str)], selected: &str, on_select: F) -> GtkBox
where
    F: Fn(&str) + 'static + Clone,
{
    let container = GtkBox::new(Orientation::Horizontal, 0);
    container.add_css_class("segmented");
    container.set_homogeneous(true);
    container.set_hexpand(true);

    for (value, label) in options {
        let value_owned = (*value).to_string();
        let selected_owned = selected.to_string();
        let button = ToggleButton::with_label(label);
        button.set_valign(gtk::Align::Center);
        if value_owned == selected_owned {
            button.set_active(true);
            button.add_css_class("selected");
        }
        let on_select = on_select.clone();
        button.connect_toggled(move |b| {
            if b.is_active() {
                on_select(&value_owned);
            }
        });
        container.append(&button);
    }
    container
}

// ---------------------------------------------------------------------------
// ConnectionBanner
// ---------------------------------------------------------------------------

pub struct ConnectionBanner;

impl ConnectionBanner {
    pub fn render(model: &AppModel) -> gtk::Widget {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.set_margin_bottom(12);
        row.set_halign(gtk::Align::Center);
        row.set_hexpand(false);
        row.add_css_class("connection-banner");

        let text = match &model.connection {
            ConnectionState::NotStarted => {
                row.add_css_class("idle");
                Label::new(Some("Pick an address to begin"))
            }
            ConnectionState::Discovering => {
                row.add_css_class("idle");
                Label::new(Some("Scanning for Bose devices…"))
            }
            ConnectionState::Connecting(addr) => {
                row.add_css_class("idle");
                Label::new(Some(&format!("Connecting to {addr}…")))
            }
            ConnectionState::Connected => Label::new(Some("Connected")),
            ConnectionState::Error(err) => {
                row.add_css_class("error");
                Label::new(Some(&format!("Connection failed: {err}")))
            }
        };
        text.set_xalign(0.5);
        row.append(&text);
        row.set_size_request(-1, 36);
        row.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// HeroCard
// ---------------------------------------------------------------------------

pub struct HeroCard;

impl HeroCard {
    pub fn render(model: &AppModel) -> gtk::Widget {
        let card = GtkBox::new(Orientation::Horizontal, 24);
        card.add_css_class("hero-card");
        card.set_hexpand(true);
        card.set_valign(gtk::Align::Center);

        let left = GtkBox::new(Orientation::Vertical, 6);
        left.set_hexpand(true);
        left.set_valign(gtk::Align::Center);

        let pulse = GtkBox::new(Orientation::Horizontal, 8);
        pulse.set_valign(gtk::Align::Center);
        let dot = GtkBox::new(Orientation::Horizontal, 0);
        dot.set_size_request(12, 12);
        dot.add_css_class("pulse");
        if !matches!(model.connection, ConnectionState::Connected) {
            dot.add_css_class("disconnected");
        }
        pulse.append(&dot);
        let status_label = Label::new(Some(match model.connection {
            ConnectionState::Connected => "CONNECTED",
            _ => "DISCONNECTED",
        }));
        status_label.set_xalign(0.0);
        status_label.add_css_class("heading");
        pulse.append(&status_label);
        left.append(&pulse);

        let (battery, name, firmware) = match &model.snapshot {
            Some(s) => (s.battery as i32, s.name.clone(), s.firmware.clone()),
            None => (-1, "—".to_string(), "—".to_string()),
        };

        let name_label = Label::new(None);
        name_label.set_markup(&format!(
            "<span weight='800' size='xx-large'>{}</span>",
            glib::markup_escape_text(&name)
        ));
        name_label.set_xalign(0.0);
        left.append(&name_label);

        let pills = GtkBox::new(Orientation::Horizontal, 6);
        let fw_pill = Label::new(Some(&format!("fw {}", firmware)));
        fw_pill.add_css_class("pill");
        pills.append(&fw_pill);

        if let Some(s) = &model.snapshot {
            let sn = Label::new(Some(&format!("SN {}", s.serial)));
            sn.add_css_class("pill");
            pills.append(&sn);

            let device_id = Label::new(Some(&format!("id 0x{:04x}", s.device_id)));
            device_id.add_css_class("pill");
            pills.append(&device_id);
        }
        pills.set_margin_top(8);
        left.append(&pills);

        card.append(&left);

        let right = GtkBox::new(Orientation::Vertical, 4);
        right.set_valign(gtk::Align::Center);
        right.set_size_request(240, -1);

        let big_number_box = GtkBox::new(Orientation::Horizontal, 0);
        big_number_box.set_halign(gtk::Align::Center);
        big_number_box.set_valign(gtk::Align::Center);
        big_number_box.set_baseline_position(gtk::BaselinePosition::Center);

        let big = Label::new(None);
        let display = if battery < 0 {
            "—".into()
        } else {
            battery.to_string()
        };
        big.set_markup(&format!(
            "<span size='7600' weight='800'>{}<span size='2800' weight='600' rise='2000'>%</span></span>",
            display
        ));
        big_number_box.append(&big);
        right.append(&big_number_box);

        let spark = build_sparkline(&model.history);
        right.append(&spark);

        let tip = Label::new(Some("last 32 polls"));
        tip.add_css_class("dim-label");
        tip.set_xalign(0.5);
        right.append(&tip);
        card.append(&right);

        card.upcast::<gtk::Widget>()
    }
}

fn build_sparkline(history: &[u8]) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("sparkline");
    row.set_homogeneous(true);
    row.set_size_request(-1, 32);

    let n = history.len().max(32);
    for i in 0..n {
        let bar = GtkBox::new(Orientation::Vertical, 0);
        bar.add_css_class("sparkline-bar");
        let h = match history.get(i) {
            Some(&v) => (v as i32).max(4),
            None => 4,
        };
        bar.set_size_request(6, h);
        if i >= history.len() {
            bar.add_css_class("empty");
        }
        row.append(&bar);
    }
    row
}

// ---------------------------------------------------------------------------
// Setting tiles
// ---------------------------------------------------------------------------

pub struct NoiseCancellingTile;

impl NoiseCancellingTile {
    pub fn render_tile_nc(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let cap = model.capabilities();
        let tile = tile("tile-nc");
        tile.append(&tile_header(
            "Noise cancelling",
            if cap.noise_cancelling {
                "active isolation"
            } else {
                "not supported on this device"
            },
        ));
        tile.set_sensitive(cap.noise_cancelling);

        let current = match model.snapshot.as_ref().map(|s| s.status.level) {
            Some(bose_connect::NoiseCancelling::High) => "high",
            Some(bose_connect::NoiseCancelling::Low) => "low",
            Some(bose_connect::NoiseCancelling::Off) => "off",
            Some(bose_connect::NoiseCancelling::Dne) => "off",
            None => "high",
        };

        let cb = {
            let s = sender.clone();
            move |v: &str| match v {
                "off" => {
                    s.send_app(AppMsg::SetNoiseCancelling(
                        bose_connect::NoiseCancelling::Off,
                    ));
                }
                "low" => {
                    s.send_app(AppMsg::SetNoiseCancelling(
                        bose_connect::NoiseCancelling::Low,
                    ));
                }
                "high" => {
                    s.send_app(AppMsg::SetNoiseCancelling(
                        bose_connect::NoiseCancelling::High,
                    ));
                }
                _ => {}
            }
        };
        let seg = segmented(
            &[("off", "Off"), ("low", "Low"), ("high", "High")],
            current,
            cb,
        );
        tile.append(&seg);

        let footer = Label::new(Some("Quieter calls and music · applied instantly"));
        footer.add_css_class("dim-label");
        footer.set_xalign(0.0);
        footer.set_margin_top(8);
        tile.append(&footer);

        tile.upcast::<gtk::Widget>()
    }

    pub fn render_tile_voice(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let tile = tile("tile-voice");
        tile.append(&tile_header("Voice prompts", "spoken status updates"));

        let active = matches!(
            model.snapshot.as_ref(),
            Some(s) if s.status.language & bose_connect::VP_ENABLE_BIT != 0
        );

        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.set_halign(gtk::Align::Center);

        let pill = Label::new(Some(if active { "On" } else { "Off" }));
        pill.add_css_class("pill");
        row.append(&pill);

        // switch
        let switch = gtk::Switch::new();
        switch.set_active(active);
        let s = sender.clone();
        switch.connect_state_set(move |_, state| {
            s.send_app(AppMsg::SetVoicePrompts(state));
            glib::Propagation::Proceed
        });
        row.append(&switch);
        tile.append(&row);
        tile.upcast::<gtk::Widget>()
    }

    pub fn render_tile_language(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let tile = tile("tile-lang");
        tile.append(&tile_header("Language", "voice-prompt language"));

        let lang_label = match &model.snapshot {
            Some(s) => {
                let byte = s.status.language & bose_connect::VP_MASK;
                match bose_connect::PromptLanguage::from_u8(byte) {
                    Some(l) => i18n::language_label(l),
                    None => "Unknown",
                }
            }
            None => "English",
        };

        let big = Label::new(None);
        big.set_markup(&format!(
            "<span size='large' weight='700'>{}</span>",
            glib::markup_escape_text(lang_label)
        ));
        tile.append(&big);

        let list = ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        for lang in crate::transport::prompt_languages() {
            let row = ListBoxRow::new();
            let h = GtkBox::new(Orientation::Horizontal, 12);
            h.set_margin_top(6);
            h.set_margin_bottom(6);
            h.set_margin_start(12);
            h.set_margin_end(12);
            let label = Label::new(Some(i18n::language_label(*lang)));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            h.append(&label);
            if matches!(&model.snapshot, Some(s) if s.status.language & bose_connect::VP_MASK == *lang as u8)
            {
                let check = Label::new(Some("✓"));
                check.add_css_class("success");
                h.append(&check);
            }
            let lang = *lang;
            let s = sender.clone();
            row.set_child(Some(&h));
            row.set_activatable(true);
            row.connect_activate(move |_| {
                s.send_app(AppMsg::SetLanguage {
                    language: lang,
                    voice_prompts: true,
                });
            });
            list.append(&row);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&list));
        scroll.set_min_content_height(160);
        scroll.set_min_content_width(140);
        scroll.set_propagate_natural_height(true);
        tile.append(&scroll);

        tile.upcast::<gtk::Widget>()
    }

    pub fn render_tile_auto_off(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let tile = tile("tile-auto-off");
        tile.append(&tile_header("Auto-off", "power saving"));

        let current = match model.snapshot.as_ref().map(|s| s.status.minutes) {
            Some(0) => bose_connect::AutoOff::Never,
            Some(5) => bose_connect::AutoOff::Min5,
            Some(20) => bose_connect::AutoOff::Min20,
            Some(40) => bose_connect::AutoOff::Min40,
            Some(60) => bose_connect::AutoOff::Min60,
            Some(180) => bose_connect::AutoOff::Min180,
            _ => bose_connect::AutoOff::Never,
        };
        let big = Label::new(None);
        big.set_markup(&format!(
            "<span size='large' weight='700'>{}</span>",
            glib::markup_escape_text(i18n::auto_off_label(current))
        ));
        tile.append(&big);

        let list = ListBox::new();
        for choice in crate::transport::auto_off_choices() {
            let r = ListBoxRow::new();
            let h = GtkBox::new(Orientation::Horizontal, 12);
            h.set_margin_top(4);
            h.set_margin_bottom(4);
            h.set_margin_start(12);
            h.set_margin_end(12);
            let label = Label::new(Some(i18n::auto_off_label(*choice)));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            h.append(&label);
            r.set_child(Some(&h));
            r.set_activatable(true);
            let choice = *choice;
            let s = sender.clone();
            r.connect_activate(move |_| {
                s.send_app(AppMsg::SetAutoOff(choice));
            });
            list.append(&r);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&list));
        scroll.set_min_content_height(150);
        tile.append(&scroll);

        tile.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// ProfilesBar
// ---------------------------------------------------------------------------

pub struct ProfilesBar;

impl ProfilesBar {
    pub fn render(_model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let wrap = tile("profiles");
        wrap.append(&tile_header("Profiles", "one-click combinations"));

        let list = ListBox::new();
        for profile in crate::services::state::Profiles::all() {
            let r = ListBoxRow::new();
            let h = GtkBox::new(Orientation::Horizontal, 12);
            h.set_margin_top(10);
            h.set_margin_bottom(10);
            h.set_margin_start(12);
            h.set_margin_end(12);
            let label = Label::new(Some(profile.label()));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.add_css_class("heading");
            h.append(&label);

            let pkg = Label::new(Some(&summarise(*profile)));
            pkg.add_css_class("dim-label");
            h.append(&pkg);
            r.set_child(Some(&h));
            r.set_activatable(true);
            let profile = *profile;
            let s = sender.clone();
            r.connect_activate(move |_| {
                s.send_app(AppMsg::ApplyProfile(profile));
            });
            list.append(&r);
        }
        wrap.append(&list);

        let footer = Label::new(Some("Profiles optimise NC, voice, auto-off, and language."));
        footer.add_css_class("dim-label");
        footer.set_xalign(0.0);
        footer.set_margin_top(8);
        wrap.append(&footer);

        wrap.upcast::<gtk::Widget>()
    }
}

fn summarise(profile: crate::services::state::ProfileName) -> String {
    let s = crate::services::state::Profiles::resolve(profile);
    let nc = s
        .noise_cancelling
        .map(i18n::noise_cancelling_label)
        .unwrap_or("—");
    let voice = s
        .voice_prompts
        .map(|v| if v { "voice on" } else { "silent" })
        .unwrap_or("—");
    let auto = s.auto_off.map(i18n::auto_off_label).unwrap_or("—");
    format!("{nc} · {voice} · {auto}")
}

// ---------------------------------------------------------------------------
// PairedDevicesPanel
// ---------------------------------------------------------------------------

pub struct PairedDevicesPanel;

impl PairedDevicesPanel {
    pub fn render(model: &AppModel) -> gtk::Widget {
        let wrap = tile("paired");
        wrap.append(&tile_header("Paired devices", "tap to connect or forget"));

        let sub = GtkBox::new(Orientation::Horizontal, 6);
        sub.set_halign(gtk::Align::Start);
        let disc_label = Label::new(Some("Pairing discoverable"));
        disc_label.add_css_class("dim-label");
        let pill = Label::new(Some("OFF"));
        pill.add_css_class("pill");
        pill.add_css_class("idle");
        sub.append(&disc_label);
        sub.append(&pill);
        wrap.append(&sub);

        let list = ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        let pairs = model
            .snapshot
            .as_ref()
            .map(|s| s.devices.clone())
            .unwrap_or_default();

        if pairs.is_empty() {
            let r = ListBoxRow::new();
            let h = GtkBox::new(Orientation::Horizontal, 12);
            h.set_margin_start(12);
            h.set_margin_end(12);
            h.set_margin_top(8);
            h.set_margin_bottom(8);
            let label = Label::new(Some("No paired devices yet"));
            label.set_xalign(0.0);
            h.append(&label);
            r.set_child(Some(&h));
            list.append(&r);
        }

        for dev in pairs {
            let r = ListBoxRow::new();
            let h = GtkBox::new(Orientation::Horizontal, 12);
            h.set_margin_top(6);
            h.set_margin_bottom(6);
            h.set_margin_start(12);
            h.set_margin_end(12);
            let card = GtkBox::new(Orientation::Horizontal, 8);
            card.add_css_class("device-card");
            if dev.status == bose_connect::DeviceStatus::Connected {
                card.add_css_class("connected");
            }
            card.set_hexpand(true);

            let name = match std::str::from_utf8(&dev.name[..dev.name_len]) {
                Ok(s) => s.to_string(),
                Err(_) => format!("Device {:02X?}", dev.address.b),
            };
            let v = GtkBox::new(Orientation::Vertical, 2);
            v.set_hexpand(true);
            let name_lbl = Label::new(Some(&name));
            name_lbl.set_xalign(0.0);
            name_lbl.add_css_class("heading");
            v.append(&name_lbl);
            let addr_lbl = Label::new(Some(&format_address(dev.address)));
            addr_lbl.set_xalign(0.0);
            addr_lbl.add_css_class("dim-label");
            v.append(&addr_lbl);
            card.append(&v);

            let pill = Label::new(Some(match dev.status {
                bose_connect::DeviceStatus::Connected => "connected",
                bose_connect::DeviceStatus::Disconnected => "paired",
                bose_connect::DeviceStatus::This => "this device",
            }));
            pill.add_css_class("pill");
            if dev.status == bose_connect::DeviceStatus::Connected {
                pill.add_css_class("success");
            }
            card.append(&pill);
            h.append(&card);
            r.set_child(Some(&h));
            list.append(&r);
        }
        wrap.append(&list);

        let discover = Button::with_label(i18n::strings::ACTION_SCAN);
        discover.add_css_class("quick-action");
        discover.set_margin_top(12);
        discover.set_halign(gtk::Align::Center);
        wrap.append(&discover);

        wrap.upcast::<gtk::Widget>()
    }
}

fn format_address(addr: bose_connect::BdAddr) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr.b[0], addr.b[1], addr.b[2], addr.b[3], addr.b[4], addr.b[5]
    )
}

// ---------------------------------------------------------------------------
// QuietModePill
// ---------------------------------------------------------------------------

pub struct QuietModePill;

impl QuietModePill {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let btn = Button::with_label(if model.quiet_mode {
            "  Quiet mode ON  "
        } else {
            "Quiet mode"
        });
        btn.add_css_class("quiet-mode");
        if model.quiet_mode {
            btn.add_css_class("active");
        }
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.send_app(AppMsg::ToggleQuietMode);
        });
        btn.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// ActivityLog
// ---------------------------------------------------------------------------

pub struct ActivityLog;

impl ActivityLog {
    pub fn render(model: &AppModel) -> gtk::Widget {
        let wrap = tile("log");
        wrap.append(&tile_header("Activity log", "what just happened"));

        let list = ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_show_separators(false);
        for entry in model.log.iter().rev().take(15) {
            let r = ListBoxRow::new();
            let h = GtkBox::new(Orientation::Horizontal, 12);
            h.set_margin_top(2);
            h.set_margin_bottom(2);
            h.add_css_class("log-row");
            h.set_hexpand(true);

            let time_lbl = Label::new(Some(&entry.timestamp.format("%H:%M:%S").to_string()));
            time_lbl.add_css_class("log-time");
            time_lbl.set_size_request(72, -1);
            h.append(&time_lbl);

            let msg_lbl = Label::new(Some(&entry.message));
            msg_lbl.set_xalign(0.0);
            msg_lbl.set_hexpand(true);
            match entry.level {
                LogLevel::Info => {}
                LogLevel::Success => msg_lbl.add_css_class("success"),
                LogLevel::Warning => msg_lbl.add_css_class("warning"),
                LogLevel::Error => msg_lbl.add_css_class("error"),
            }
            h.append(&msg_lbl);
            r.set_child(Some(&h));
            list.append(&r);
        }
        wrap.append(&list);

        wrap.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// App menu (Gio::Menu)
// ---------------------------------------------------------------------------

pub fn build_app_menu(app: &gtk::Application) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    let section = gtk::gio::Menu::new();
    section.append(Some("About Bose Connect for Linux"), Some("app.about"));
    section.append(Some("Visit the project page"), Some("app.project"));
    section.append(Some("Quit"), Some("app.quit"));
    menu.append_section(None, &section);

    let about = gtk::gio::SimpleAction::new("about", None);
    about.connect_activate(|_, _| tracing::info!("about action triggered"));
    app.add_action(&about);

    let project = gtk::gio::SimpleAction::new("project", None);
    project.connect_activate(|_, _| tracing::info!("project action triggered"));
    app.add_action(&project);

    let quit = gtk::gio::SimpleAction::new("quit", None);
    let app_weak = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit);

    menu
}

// Unused: adw re-export for callers that import from widgets.
#[allow(unused_imports)]
use adw as _adw_unused;
