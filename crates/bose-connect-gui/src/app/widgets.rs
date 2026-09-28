//! Widget sub-trees — pure functions that take `&AppModel` and a
//! message-sending handle and return a fully-formed widget.
//!
//! Layout follows the Claude / GNOME-sidebar mockup:
//!
//! ```text
//!     ┌─ HeaderBar ────────────────────────────────────────────┐
//!     │ [Bose Connect]   [device ▼]            [🌓] [☰]         │
//!     └────────────────────────────────────────────────────────┘
//!     ┌─ Sidebar ──┬─ Page content ─────────────────────────────┐
//!     │ My Bose    │                                            │
//!     │ Overview   │  <page-specific widgets>                   │
//!     │ Audio      │                                            │
//!     │ Device     │                                            │
//!     │ Multipoint │                                            │
//!     │ Advanced   │                                            │
//!     └────────────┴────────────────────────────────────────────┘
//! ```
//!
//! Each page is rendered by one of the public functions at the
//! bottom of this file. We keep the small reusable widgets
//! (hero card, segmented control, action row, boxed list, …)
//! private helpers above them.

use adw;
use adw::prelude::*;
use gtk::prelude::*;
use gtk::{glib, Box as GtkBox, Button, Label, ListBox, ListBoxRow, Orientation};

use crate::app::model::{AppModel, AppMsg, ConnectionState, LogLevel, Page};
use crate::i18n;

// ---------------------------------------------------------------------------
// Sender extension
// ---------------------------------------------------------------------------

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

/// A vertical box styled as one of the project's "tile" cards.
/// Used for the quick-settings cards on the audio / device pages.
#[allow(dead_code)]
fn tile(class: &str) -> GtkBox {
    let b = GtkBox::new(Orientation::Vertical, 8);
    b.add_css_class("tile");
    if !class.is_empty() {
        b.add_css_class(class);
    }
    b.set_hexpand(true);
    b.set_vexpand(true);
    b
}

#[allow(dead_code)]
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

/// A horizontal segmented selector (Off / Low / High) using
/// `ToggleButton`s. The selected option gets the `selected`
/// CSS class so it picks up the accent background.
fn segmented_control<F>(options: &[(&str, &str)], selected: &str, on_select: F) -> GtkBox
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
        let button = gtk::ToggleButton::with_label(label);
        button.add_css_class("segmented-button");
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

/// Build a "boxed list" — the libadwaita / GNOME settings card
/// with rounded corners that contains action rows separated by
/// thin borders.
fn boxed_list() -> ListBox {
    let list = ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_show_separators(true);
    list
}

/// Build a settings-row-style action row used inside a boxed
/// list. `prefix` is a leading icon glyph; `title` / `desc` are
/// the main and secondary labels; `suffix` is the trailing
/// widget (a button, a switch, a chevron, etc.).
fn action_row<W: gtk::prelude::IsA<gtk::Widget>>(
    prefix: &str,
    title: &str,
    desc: &str,
    suffix: &W,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("action-row");
    let h = GtkBox::new(Orientation::Horizontal, 12);
    h.set_margin_top(8);
    h.set_margin_bottom(8);
    h.set_margin_start(12);
    h.set_margin_end(12);

    if !prefix.is_empty() {
        let ic = Label::new(Some(prefix));
        ic.add_css_class("row-icon");
        ic.set_size_request(20, -1);
        ic.set_xalign(0.0);
        h.append(&ic);
    }

    let texts = GtkBox::new(Orientation::Vertical, 2);
    texts.set_hexpand(true);
    let title_lbl = Label::new(Some(title));
    title_lbl.add_css_class("row-title");
    title_lbl.set_xalign(0.0);
    texts.append(&title_lbl);
    if !desc.is_empty() {
        let desc_lbl = Label::new(Some(desc));
        desc_lbl.add_css_class("row-desc");
        desc_lbl.set_xalign(0.0);
        desc_lbl.set_wrap(true);
        texts.append(&desc_lbl);
    }
    h.append(&texts);
    h.append(suffix);
    row.set_child(Some(&h));
    row
}

fn format_address(addr: bose_connect::BdAddr) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr.b[0], addr.b[1], addr.b[2], addr.b[3], addr.b[4], addr.b[5]
    )
}

// ---------------------------------------------------------------------------
// ConnectionBanner (slim status strip — used at the top of every page)
// ---------------------------------------------------------------------------

pub struct ConnectionBanner;

impl ConnectionBanner {
    pub fn render(model: &AppModel) -> gtk::Widget {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.set_margin_bottom(8);
        row.set_halign(gtk::Align::Start);
        row.set_hexpand(true);
        row.add_css_class("connection-banner");

        let dot = GtkBox::new(Orientation::Horizontal, 0);
        dot.set_size_request(10, 10);
        dot.add_css_class("status-dot");
        match model.connection {
            ConnectionState::Connected => dot.add_css_class("status-dot-on"),
            ConnectionState::Error(_) => dot.add_css_class("status-dot-error"),
            _ => dot.add_css_class("status-dot-idle"),
        }
        row.append(&dot);

        let text = match &model.connection {
            ConnectionState::NotStarted => Label::new(Some("Pick a device to begin")),
            ConnectionState::Discovering => Label::new(Some("Scanning for Bose devices…")),
            ConnectionState::Connecting(addr) => {
                Label::new(Some(&format!("Connecting to {addr}…")))
            }
            ConnectionState::Connected => Label::new(Some("Connected via RFCOMM")),
            ConnectionState::Error(err) => Label::new(Some(&format!("Connection failed: {err}"))),
        };
        text.set_xalign(0.0);
        text.add_css_class("status-label");
        row.append(&text);
        row.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// HeroCard (used on the Overview page)
// ---------------------------------------------------------------------------

pub struct HeroCard;

impl HeroCard {
    pub fn render(model: &AppModel) -> gtk::Widget {
        let card = GtkBox::new(Orientation::Horizontal, 20);
        card.add_css_class("hero-card");
        card.set_hexpand(true);
        card.set_valign(gtk::Align::Center);

        let (battery, name, firmware) = match &model.snapshot {
            Some(s) => (s.battery as i32, s.name.clone(), s.firmware.clone()),
            None => (-1, "—".to_string(), "—".to_string()),
        };

        // Left: avatar + identity.
        let left = GtkBox::new(Orientation::Horizontal, 16);
        left.set_hexpand(true);
        left.set_valign(gtk::Align::Center);

        let avatar = GtkBox::new(Orientation::Horizontal, 0);
        avatar.add_css_class("hero-avatar");
        avatar.set_size_request(56, 56);
        avatar.set_valign(gtk::Align::Center);
        avatar.set_halign(gtk::Align::Center);
        let avatar_label = Label::new(Some("🎧"));
        avatar_label
            .set_markup("<span font_features='liga' size='20000' foreground='#ffffff'>🎧</span>");
        avatar.append(&avatar_label);
        left.append(&avatar);

        let meta = GtkBox::new(Orientation::Vertical, 4);
        meta.set_hexpand(true);
        let name_label = Label::new(None);
        name_label.set_markup(&format!(
            "<span weight='800' size='large'>{}</span>",
            glib::markup_escape_text(&name)
        ));
        name_label.set_xalign(0.0);
        meta.append(&name_label);

        let sub = Label::new(Some(&format!("Firmware {} · AA:BB:CC:DD:EE:FF", firmware)));
        sub.set_xalign(0.0);
        sub.add_css_class("row-desc");
        meta.append(&sub);

        let pills = GtkBox::new(Orientation::Horizontal, 6);
        let status_pill = Label::new(Some(match model.connection {
            ConnectionState::Connected => "● connected",
            _ => "○ disconnected",
        }));
        status_pill.add_css_class("pill");
        if matches!(model.connection, ConnectionState::Connected) {
            status_pill.add_css_class("success");
        } else {
            status_pill.add_css_class("idle");
        }
        pills.append(&status_pill);
        meta.append(&pills);
        left.append(&meta);

        card.append(&left);

        // Right: battery ring + sparkline.
        let right = GtkBox::new(Orientation::Vertical, 6);
        right.set_valign(gtk::Align::Center);
        right.set_size_request(220, -1);

        let ring = GtkBox::new(Orientation::Horizontal, 0);
        ring.add_css_class("battery-ring");
        ring.set_size_request(96, 96);
        ring.set_halign(gtk::Align::Center);
        ring.set_valign(gtk::Align::Center);
        let pct = if battery < 0 {
            "—".to_string()
        } else {
            format!("{battery}<span size='40%' rise='20'>%</span>")
        };
        let ring_label = Label::new(None);
        ring_label.set_markup(&format!("<span weight='800' size='17000'>{pct}</span>"));
        ring.append(&ring_label);
        right.append(&ring);

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
    row.set_size_request(-1, 28);

    let n = history.len().max(32);
    for i in 0..n {
        let bar = GtkBox::new(Orientation::Vertical, 0);
        bar.add_css_class("sparkline-bar");
        let h = match history.get(i) {
            Some(&v) => (v as i32).max(4),
            None => 4,
        };
        bar.set_size_request(5, h);
        if i >= history.len() {
            bar.add_css_class("empty");
        }
        row.append(&bar);
    }
    row
}

// ---------------------------------------------------------------------------
// MyBose page (sidebar entry 1)
// ---------------------------------------------------------------------------

pub struct MyBosePage;

impl MyBosePage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 16);
        page.set_hexpand(true);
        page.set_vexpand(true);
        page.append(&page_header(Page::MyBose));

        // Active device card.
        let active_card = adw::PreferencesGroup::new();
        active_card.set_title("Active device");
        active_card.set_description(Some("The Bose device this app is currently controlling"));

        let active_list = boxed_list();
        let (name, addr_str, battery, status_str) = match &model.snapshot {
            Some(s) => (
                s.name.clone(),
                format_address(s.address),
                format!("{}%", s.battery),
                "Connected",
            ),
            None => (
                "No device selected".to_string(),
                "—".to_string(),
                "—".to_string(),
                "Disconnected",
            ),
        };
        let suffix = GtkBox::new(Orientation::Horizontal, 6);
        let pill = Label::new(Some(status_str));
        pill.add_css_class("pill");
        if status_str == "Connected" {
            pill.add_css_class("success");
        } else {
            pill.add_css_class("idle");
        }
        suffix.append(&pill);
        let disconnect = Button::with_label("Disconnect");
        disconnect.add_css_class("destructive");
        let s = sender.clone();
        disconnect.connect_clicked(move |_| {
            // In the mock we just toggle connection state via log;
            // a real RFCOMM disconnect would map to a dedicated
            // command. The mock currently doesn't expose a
            // "disconnect" verb, so we just leave a log line.
            s.send_app(AppMsg::Acknowledge);
        });
        suffix.append(&disconnect);
        let row = action_row("🎧", &name, &addr_str, &suffix.upcast::<gtk::Widget>());
        row.set_activatable(false);
        active_list.append(&row);
        let battery_value = Label::new(Some(&battery));
        battery_value.add_css_class("row-value");
        let battery_row = action_row(
            "🔋",
            "Battery level",
            "Live reading from the connected device",
            &battery_value,
        );
        battery_row.set_activatable(false);
        active_list.append(&battery_row);
        active_card.add(&active_list);
        page.append(&active_card);

        // Paired list (the laptop <-> Bose side of the pairing).
        let paired = adw::PreferencesGroup::new();
        paired.set_title("Paired on this laptop");
        paired.set_description(Some("Devices the laptop has previously paired with"));
        let paired_list = boxed_list();

        // Mock data for the prototype — RealService would walk
        // BlueZ and pull the persisted list.
        let mock_devices = vec![
            (
                "Bose SoundLink Revolve",
                "BB:CC:DD:EE:FF:00",
                "Trusted",
                "Connect",
            ),
            (
                "Bose QuietComfort Earbuds",
                "CC:DD:EE:FF:00:11",
                "Paused",
                "Use this",
            ),
        ];
        for (name, addr, tag, action) in mock_devices {
            let suffix = GtkBox::new(Orientation::Horizontal, 6);
            let pill = Label::new(Some(tag));
            pill.add_css_class("pill");
            pill.add_css_class("muted");
            suffix.append(&pill);
            let btn = Button::with_label(action);
            btn.add_css_class("suggested-action");
            let s = sender.clone();
            btn.connect_clicked(move |_| {
                s.send_app(AppMsg::Acknowledge);
            });
            suffix.append(&btn);
            let forget = Button::with_label("Forget");
            forget.add_css_class("destructive");
            forget.add_css_class("small");
            suffix.append(&forget);
            let row = action_row("🎵", name, addr, &suffix.upcast::<gtk::Widget>());
            row.set_activatable(false);
            paired_list.append(&row);
        }
        paired.add(&paired_list);
        page.append(&paired);

        // Discovery hint.
        let hint = adw::PreferencesGroup::new();
        hint.set_title("Add a new Bose device");
        hint.set_description(Some(
            "Make the headphones discoverable, then scan from this app",
        ));
        let scan_row = GtkBox::new(Orientation::Horizontal, 12);
        scan_row.set_margin_top(8);
        scan_row.set_margin_bottom(8);
        scan_row.set_margin_start(12);
        scan_row.set_margin_end(12);
        let scan_label = Label::new(Some("Scan for Bose devices nearby"));
        scan_label.set_xalign(0.0);
        scan_label.set_hexpand(true);
        scan_row.append(&scan_label);
        let scan_btn = Button::with_label("Scan");
        scan_btn.add_css_class("suggested-action");
        let s = sender.clone();
        scan_btn.connect_clicked(move |_| {
            s.send_app(AppMsg::Acknowledge);
        });
        scan_row.append(&scan_btn);
        let card = GtkBox::new(Orientation::Vertical, 0);
        card.add_css_class("boxed-card");
        card.append(&scan_row);
        hint.add(&card);
        page.append(&hint);

        page.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// Overview page (sidebar entry 2)
// ---------------------------------------------------------------------------

pub struct OverviewPage;

impl OverviewPage {
    pub fn render(model: &AppModel, _sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 16);
        page.set_hexpand(true);
        page.set_vexpand(true);
        page.append(&page_header(Page::Overview));

        page.append(&HeroCard::render(model));

        // Quick actions.
        let actions = adw::PreferencesGroup::new();
        actions.set_title("Quick actions");
        let list = boxed_list();
        let reconnect_btn = Button::with_label("Connect");
        let reconnect = action_row(
            "🔌",
            "Reconnect device",
            "Force a fresh RFCOMM handshake",
            &reconnect_btn,
        );
        reconnect.set_activatable(false);
        list.append(&reconnect);
        let disconnect_btn = Button::with_label("Disconnect");
        disconnect_btn.add_css_class("destructive");
        let disconnect = action_row(
            "⛔",
            "Disconnect",
            "Close the active RFCOMM session",
            &disconnect_btn,
        );
        disconnect.set_activatable(false);
        list.append(&disconnect);
        actions.add(&list);
        page.append(&actions);

        // Device info.
        let info = adw::PreferencesGroup::new();
        info.set_title("Device information");
        let ilist = boxed_list();
        let (device_id, serial, firmware) = match &model.snapshot {
            Some(s) => (
                format!("0x{:04X} · rev {}", s.device_id, 2),
                s.serial.clone(),
                s.firmware.clone(),
            ),
            None => ("—".to_string(), "—".to_string(), "—".to_string()),
        };
        let id_value = Label::new(Some(&device_id));
        id_value.add_css_class("row-value");
        let id_row = action_row(
            "🏷️",
            "Device ID",
            "Hardware revision reported by the firmware",
            &id_value,
        );
        id_row.set_activatable(false);
        ilist.append(&id_row);
        let sn_value = Label::new(Some(&serial));
        sn_value.add_css_class("row-value");
        let sn_row = action_row(
            "🔢",
            "Serial number",
            "Unique hardware identifier",
            &sn_value,
        );
        sn_row.set_activatable(false);
        ilist.append(&sn_row);
        let fw_value = Label::new(Some(&firmware));
        fw_value.add_css_class("row-value");
        let fw_row = action_row(
            "💾",
            "Firmware version",
            "Currently running firmware",
            &fw_value,
        );
        fw_row.set_activatable(false);
        ilist.append(&fw_row);
        info.add(&ilist);
        page.append(&info);

        page.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// Audio page (sidebar entry 3)
// ---------------------------------------------------------------------------

pub struct AudioPage;

impl AudioPage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 16);
        page.set_hexpand(true);
        page.set_vexpand(true);
        page.append(&page_header(Page::Audio));

        let cap = model.capabilities();
        // ANC
        let anc_group = adw::PreferencesGroup::new();
        anc_group.set_title("Noise cancellation");
        anc_group.set_description(Some("How much outside noise the headphones should block"));
        let current = match model.snapshot.as_ref().map(|s| s.status.level) {
            Some(bose_connect::NoiseCancelling::High) => "high",
            Some(bose_connect::NoiseCancelling::Low) => "low",
            _ => "off",
        };
        let seg_inner = GtkBox::new(Orientation::Horizontal, 0);
        seg_inner.set_margin_top(8);
        seg_inner.set_margin_bottom(8);
        seg_inner.set_margin_start(12);
        seg_inner.set_margin_end(12);
        let cb = {
            let s = sender.clone();
            move |v: &str| match v {
                "off" => s.send_app(AppMsg::SetNoiseCancelling(
                    bose_connect::NoiseCancelling::Off,
                )),
                "low" => s.send_app(AppMsg::SetNoiseCancelling(
                    bose_connect::NoiseCancelling::Low,
                )),
                "high" => s.send_app(AppMsg::SetNoiseCancelling(
                    bose_connect::NoiseCancelling::High,
                )),
                _ => {}
            }
        };
        let seg_widget = segmented_control(
            &[("off", "Off"), ("low", "Low"), ("high", "High")],
            current,
            cb,
        );
        seg_inner.append(&seg_widget);
        let card = GtkBox::new(Orientation::Vertical, 0);
        card.add_css_class("boxed-card");
        card.append(&seg_inner);
        anc_group.add(&card);
        anc_group.set_sensitive(cap.noise_cancelling);
        page.append(&anc_group);

        // Self-voice (sidetone) slider-ish row using 4 options.
        let sv_group = adw::PreferencesGroup::new();
        sv_group.set_title("Self-voice (sidetone)");
        sv_group.set_description(Some("How loudly you hear your own voice during calls"));
        let sv_current = match model.snapshot.as_ref().map(|s| s.status.level) {
            Some(_) => "medium", // mock doesn't track self-voice separately; default to medium
            None => "off",
        };
        let sv_card = GtkBox::new(Orientation::Vertical, 0);
        sv_card.add_css_class("boxed-card");
        let sv_inner = GtkBox::new(Orientation::Horizontal, 0);
        sv_inner.set_margin_top(8);
        sv_inner.set_margin_bottom(8);
        sv_inner.set_margin_start(12);
        sv_inner.set_margin_end(12);
        let sv_cb = {
            let s = sender.clone();
            move |v: &str| {
                let level = match v {
                    "off" => bose_connect::SelfVoice::Off,
                    "low" => bose_connect::SelfVoice::Low,
                    "medium" => bose_connect::SelfVoice::Medium,
                    "high" => bose_connect::SelfVoice::High,
                    _ => bose_connect::SelfVoice::Off,
                };
                s.send_app(AppMsg::SetSelfVoice(level));
            }
        };
        let sv_widget = segmented_control(
            &[
                ("off", "Off"),
                ("low", "Low"),
                ("medium", "Medium"),
                ("high", "High"),
            ],
            sv_current,
            sv_cb,
        );
        sv_inner.append(&sv_widget);
        sv_card.append(&sv_inner);
        sv_group.add(&sv_card);
        page.append(&sv_group);

        page.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// Device page (sidebar entry 4)
// ---------------------------------------------------------------------------

pub struct DevicePage;

impl DevicePage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 16);
        page.set_hexpand(true);
        page.set_vexpand(true);
        page.append(&page_header(Page::Device));

        let group = adw::PreferencesGroup::new();
        group.set_title("Identity");
        let list = boxed_list();

        // Device name (inline editable row).
        let name_entry = gtk::Entry::new();
        name_entry.set_text(
            model
                .snapshot
                .as_ref()
                .map(|s| s.name.as_str())
                .unwrap_or(""),
        );
        name_entry.set_max_width_chars(20);
        let s = sender.clone();
        name_entry.connect_changed(move |e| {
            let text = e.text().to_string();
            s.send_app(AppMsg::SetName(text));
        });
        let name_row = action_row(
            "✏️",
            "Device name",
            "The friendly name broadcast over Bluetooth",
            &name_entry.upcast::<gtk::Widget>(),
        );
        name_row.set_activatable(false);
        list.append(&name_row);

        // Auto-off dropdown (modeled with a button-row popover-less
        // — we use a `DropDown` from libadwaita / gtk4-dropdown).
        let current_auto = match model.snapshot.as_ref().map(|s| s.status.minutes) {
            Some(0) => "Never",
            Some(5) => "5 min",
            Some(20) => "20 min",
            Some(40) => "40 min",
            Some(60) => "60 min",
            Some(180) => "180 min",
            _ => "Never",
        };
        let choices: Vec<&str> = vec!["Never", "5 min", "20 min", "40 min", "60 min", "180 min"];
        let dropdown = gtk::DropDown::from_strings(&choices);
        if let Some(idx) = choices.iter().position(|c| *c == current_auto) {
            dropdown.set_selected(idx as u32);
        }
        let s = sender.clone();
        dropdown.connect_selected_notify(move |d| {
            let idx = d.selected() as usize;
            let picked = choices.get(idx).copied().unwrap_or("Never");
            let mins = match picked {
                "Never" => bose_connect::AutoOff::Never,
                "5 min" => bose_connect::AutoOff::Min5,
                "20 min" => bose_connect::AutoOff::Min20,
                "40 min" => bose_connect::AutoOff::Min40,
                "60 min" => bose_connect::AutoOff::Min60,
                "180 min" => bose_connect::AutoOff::Min180,
                _ => bose_connect::AutoOff::Never,
            };
            s.send_app(AppMsg::SetAutoOff(mins));
        });
        let ao_row = action_row(
            "⏱️",
            "Auto-off",
            "Turn off the device after this much idle time",
            &dropdown.upcast::<gtk::Widget>(),
        );
        ao_row.set_activatable(false);
        list.append(&ao_row);

        // Language dropdown.
        let lang_choices: Vec<&str> = vec![
            "English",
            "French",
            "Italian",
            "German",
            "Spanish",
            "Portuguese",
            "Chinese",
            "Korean",
            "Russian",
            "Polish",
            "Dutch",
            "Japanese",
            "Swedish",
        ];
        let current_lang = match model.snapshot.as_ref() {
            Some(s) => {
                let byte = s.status.language & bose_connect::VP_MASK;
                i18n::language_label(
                    bose_connect::PromptLanguage::from_u8(byte)
                        .unwrap_or(bose_connect::PromptLanguage::En),
                )
            }
            None => "English",
        };
        let dropdown_lang = gtk::DropDown::from_strings(&lang_choices);
        if let Some(idx) = lang_choices.iter().position(|c| *c == current_lang) {
            dropdown_lang.set_selected(idx as u32);
        }
        let s = sender.clone();
        dropdown_lang.connect_selected_notify(move |d| {
            let idx = d.selected() as usize;
            let picked = lang_choices.get(idx).copied().unwrap_or("English");
            let lang = match picked {
                "English" => bose_connect::PromptLanguage::En,
                "French" => bose_connect::PromptLanguage::Fr,
                "Italian" => bose_connect::PromptLanguage::It,
                "German" => bose_connect::PromptLanguage::De,
                "Spanish" => bose_connect::PromptLanguage::Es,
                "Portuguese" => bose_connect::PromptLanguage::Pt,
                "Chinese" => bose_connect::PromptLanguage::Zh,
                "Korean" => bose_connect::PromptLanguage::Ko,
                "Russian" => bose_connect::PromptLanguage::Ru,
                "Polish" => bose_connect::PromptLanguage::Pl,
                "Dutch" => bose_connect::PromptLanguage::Nl,
                "Japanese" => bose_connect::PromptLanguage::Ja,
                "Swedish" => bose_connect::PromptLanguage::Sv,
                _ => bose_connect::PromptLanguage::En,
            };
            s.send_app(AppMsg::SetLanguage {
                language: lang,
                voice_prompts: true,
            });
        });
        let lang_row = action_row(
            "🌐",
            "Voice prompt language",
            "Spoken feedback when the device announces state",
            &dropdown_lang.upcast::<gtk::Widget>(),
        );
        lang_row.set_activatable(false);
        list.append(&lang_row);

        // Voice prompts switch.
        let voice_on = matches!(
            model.snapshot.as_ref(),
            Some(s) if s.status.language & bose_connect::VP_ENABLE_BIT != 0
        );
        let switch_vp = gtk::Switch::new();
        switch_vp.set_active(voice_on);
        switch_vp.set_valign(gtk::Align::Center);
        let s = sender.clone();
        switch_vp.connect_state_set(move |_, state| {
            s.send_app(AppMsg::SetVoicePrompts(state));
            glib::Propagation::Proceed
        });
        let vp_row = action_row(
            "🗣️",
            "Voice prompts",
            "Spoken feedback when the device announces state",
            &switch_vp.upcast::<gtk::Widget>(),
        );
        vp_row.set_activatable(false);
        list.append(&vp_row);

        // Pairing visibility switch.
        let pairing_on = matches!(
            model.snapshot.as_ref(),
            Some(s) if s.paired.connected == bose_connect::DevicesConnected::One
                || s.paired.connected == bose_connect::DevicesConnected::Two
        );
        // We can't tell pairing_on from the snapshot easily; use
        // model state via mock. The snapshot doesn't carry it
        // today; default to false.
        let _ = pairing_on;
        let switch_pair = gtk::Switch::new();
        switch_pair.set_valign(gtk::Align::Center);
        let s = sender.clone();
        switch_pair.connect_state_set(move |_, state| {
            s.send_app(AppMsg::SetPairing(state));
            glib::Propagation::Proceed
        });
        let pair_row = action_row(
            "📡",
            "Visible to pair",
            "Allow other devices to discover this Bose",
            &switch_pair.upcast::<gtk::Widget>(),
        );
        pair_row.set_activatable(false);
        list.append(&pair_row);

        group.add(&list);
        page.append(&group);

        // Quiet mode card.
        let qm = adw::PreferencesGroup::new();
        qm.set_title("Quiet mode");
        qm.set_description(Some(
            "One-tap combo: max noise-cancelling, voice off, never auto-off",
        ));
        let qm_card = GtkBox::new(Orientation::Horizontal, 12);
        qm_card.set_margin_top(8);
        qm_card.set_margin_bottom(8);
        qm_card.set_margin_start(12);
        qm_card.set_margin_end(12);
        let qm_label = Label::new(Some(if model.quiet_mode {
            "Quiet mode is on"
        } else {
            "Quick apply the focus profile"
        }));
        qm_label.set_xalign(0.0);
        qm_label.set_hexpand(true);
        qm_card.append(&qm_label);
        let qm_btn = Button::with_label(if model.quiet_mode {
            "Turn off"
        } else {
            "Enable"
        });
        qm_btn.add_css_class("suggested-action");
        let s = sender.clone();
        qm_btn.connect_clicked(move |_| {
            s.send_app(AppMsg::ToggleQuietMode);
        });
        qm_card.append(&qm_btn);
        let card = GtkBox::new(Orientation::Vertical, 0);
        card.add_css_class("boxed-card");
        card.append(&qm_card);
        qm.add(&card);
        page.append(&qm);

        page.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// Multipoint page (sidebar entry 5)
// ---------------------------------------------------------------------------

pub struct MultipointPage;

impl MultipointPage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 16);
        page.set_hexpand(true);
        page.set_vexpand(true);
        page.append(&page_header(Page::Multipoint));

        let group = adw::PreferencesGroup::new();
        group.set_title("Devices using this Bose");
        group.set_description(Some("Other machines paired with the *same* headset"));
        let list = boxed_list();
        let pairs = model
            .snapshot
            .as_ref()
            .map(|s| s.devices.clone())
            .unwrap_or_default();
        if pairs.is_empty() {
            let placeholder = Label::new(Some(""));
            let row = action_row(
                "📡",
                "No multipoint devices yet",
                "Enable 'Visible to pair' from the Device page",
                &placeholder,
            );
            row.set_activatable(false);
            list.append(&row);
        }
        for dev in pairs {
            let name = match std::str::from_utf8(&dev.name[..dev.name_len]) {
                Ok(s) => s.to_string(),
                Err(_) => format!("Device {:02X?}", dev.address.b),
            };
            let (tag, is_active) = match dev.status {
                bose_connect::DeviceStatus::This => ("This device", true),
                bose_connect::DeviceStatus::Connected => ("Connected", true),
                bose_connect::DeviceStatus::Disconnected => ("Paired", false),
            };
            let suffix = GtkBox::new(Orientation::Horizontal, 6);
            let pill = Label::new(Some(tag));
            pill.add_css_class("pill");
            if is_active {
                pill.add_css_class("success");
            } else {
                pill.add_css_class("muted");
            }
            suffix.append(&pill);
            if is_active {
                let dc = Button::with_label("Disconnect");
                dc.add_css_class("small");
                dc.add_css_class("destructive");
                let s = sender.clone();
                dc.connect_clicked(move |_| {
                    s.send_app(AppMsg::PairedDisconnect(dev.address));
                });
                suffix.append(&dc);
            } else {
                let cn = Button::with_label("Connect");
                cn.add_css_class("small");
                cn.add_css_class("suggested-action");
                let s = sender.clone();
                cn.connect_clicked(move |_| {
                    s.send_app(AppMsg::PairedConnect(dev.address));
                });
                suffix.append(&cn);
            }
            let del = Button::with_label("Delete");
            del.add_css_class("small");
            del.add_css_class("destructive");
            let s = sender.clone();
            del.connect_clicked(move |_| {
                s.send_app(AppMsg::PairedRemove(dev.address));
            });
            suffix.append(&del);
            let row = action_row(
                "💻",
                &name,
                &format_address(dev.address),
                &suffix.upcast::<gtk::Widget>(),
            );
            row.set_activatable(false);
            list.append(&row);
        }
        group.add(&list);

        let add_row = GtkBox::new(Orientation::Horizontal, 12);
        add_row.set_margin_top(8);
        add_row.set_margin_bottom(8);
        add_row.set_margin_start(12);
        add_row.set_margin_end(12);
        let add_label = Label::new(Some("Add another device to this Bose"));
        add_label.set_xalign(0.0);
        add_label.set_hexpand(true);
        add_row.append(&add_label);
        let add_btn = Button::with_label("+ Add device");
        add_btn.add_css_class("suggested-action");
        let s = sender.clone();
        add_btn.connect_clicked(move |_| {
            s.send_app(AppMsg::Acknowledge);
        });
        add_row.append(&add_btn);
        let card = GtkBox::new(Orientation::Vertical, 0);
        card.add_css_class("boxed-card");
        card.append(&add_row);
        group.add(&card);

        page.append(&group);
        page.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// Advanced page (sidebar entry 6)
// ---------------------------------------------------------------------------

pub struct AdvancedPage;

impl AdvancedPage {
    pub fn render(_model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 16);
        page.set_hexpand(true);
        page.set_vexpand(true);
        page.append(&page_header(Page::Advanced));

        let group = adw::PreferencesGroup::new();
        group.set_title("RFCOMM debugger");
        group.set_description(Some(
            "Raw hex packet passthrough — useful when reverse-engineering the protocol",
        ));
        let list = boxed_list();

        // Hex packet sender.
        let input_row = GtkBox::new(Orientation::Horizontal, 8);
        input_row.set_margin_top(8);
        input_row.set_margin_bottom(8);
        input_row.set_margin_start(12);
        input_row.set_margin_end(12);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("0a1b2c3d"));
        entry.set_hexpand(true);
        input_row.append(&entry);
        let send = Button::with_label("Send");
        send.add_css_class("suggested-action");
        let s = sender.clone();
        send.connect_clicked(move |_| {
            s.send_app(AppMsg::Acknowledge);
        });
        input_row.append(&send);
        let input_card = GtkBox::new(Orientation::Vertical, 0);
        input_card.add_css_class("boxed-card");
        input_card.append(&input_row);

        let resp_box = GtkBox::new(Orientation::Vertical, 4);
        resp_box.set_margin_top(8);
        resp_box.set_margin_bottom(8);
        resp_box.set_margin_start(12);
        resp_box.set_margin_end(12);
        let resp_label = Label::new(Some("Response:"));
        resp_label.set_xalign(0.0);
        resp_label.add_css_class("row-desc");
        resp_box.append(&resp_label);
        let resp = Label::new(Some("01 00 04 00 aa bb cc dd"));
        resp.set_xalign(0.0);
        resp.set_selectable(true);
        resp.add_css_class("mono");
        resp_box.append(&resp);
        let resp_card = GtkBox::new(Orientation::Vertical, 0);
        resp_card.add_css_class("boxed-card");
        resp_card.append(&resp_box);

        let row = action_row(
            "📟",
            "Send hex packet",
            "Use the field below to send a raw frame",
            &input_card.upcast::<gtk::Widget>(),
        );
        row.set_activatable(false);
        list.append(&row);
        let row_resp = action_row(
            "📥",
            "Last response",
            "Hex bytes returned by the device",
            &resp_card.upcast::<gtk::Widget>(),
        );
        row_resp.set_activatable(false);
        list.append(&row_resp);
        group.add(&list);
        page.append(&group);

        // Activity log on advanced page (matches the existing
        // debug surface from the original build).
        let log_group = adw::PreferencesGroup::new();
        log_group.set_title("Activity log");
        log_group.set_description(Some("What just happened"));
        let log_list = boxed_list();
        let entries = render_log_entries(_model);
        for entry in entries {
            log_list.append(&entry);
        }
        log_group.add(&log_list);
        page.append(&log_group);

        page.upcast::<gtk::Widget>()
    }
}

fn render_log_entries(model: &AppModel) -> Vec<ListBoxRow> {
    let mut rows = Vec::new();
    for entry in model.log.iter().rev().take(15) {
        let row = ListBoxRow::new();
        let h = GtkBox::new(Orientation::Horizontal, 12);
        h.set_margin_top(4);
        h.set_margin_bottom(4);
        h.set_margin_start(12);
        h.set_margin_end(12);
        let time_lbl = Label::new(Some(&entry.timestamp.format("%H:%M:%S").to_string()));
        time_lbl.add_css_class("row-desc");
        time_lbl.set_size_request(72, -1);
        time_lbl.set_xalign(0.0);
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
        row.set_child(Some(&h));
        rows.push(row);
    }
    rows
}

// ---------------------------------------------------------------------------
// Page header (used at the top of every page)
// ---------------------------------------------------------------------------

fn page_header(page: Page) -> gtk::Widget {
    let header = GtkBox::new(Orientation::Vertical, 2);
    header.set_margin_bottom(4);
    let title = Label::new(None);
    title.set_markup(&format!(
        "<span weight='800' size='17000'>{}</span>",
        page.title()
    ));
    title.set_xalign(0.0);
    header.append(&title);
    let sub = Label::new(Some(page.subtitle()));
    sub.set_xalign(0.0);
    sub.add_css_class("row-desc");
    header.append(&sub);
    header.upcast::<gtk::Widget>()
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

pub struct Sidebar;

impl Sidebar {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let (widget, _list) = Self::render_with_list(model, sender);
        widget
    }

    /// Variant of `render` that hands the inner `ListBox` back so
    /// callers (specifically the `RenderHandle`) can update the
    /// `.selected` CSS class when `current_page` changes after a
    /// `NavigateTo` message.
    pub fn render_with_list(
        model: &AppModel,
        sender: &flume::Sender<AppMsg>,
    ) -> (gtk::Widget, gtk::ListBox) {
        let list = ListBox::new();
        list.add_css_class("sidebar-list");
        list.set_selection_mode(gtk::SelectionMode::Single);
        // GTK4's default for `activate-on-single-click` is FALSE,
        // so a single click on a row only *selects* it (which we
        // already see as the highlight) without firing
        // `row-activated`. We need the activated signal so we can
        // route clicks to `AppMsg::NavigateTo`. Set the property
        // explicitly here.
        list.set_activate_on_single_click(true);

        for page in Page::ALL {
            let row = ListBoxRow::new();
            row.add_css_class("sidebar-row");
            row.set_widget_name(page.key());
            if *page == model.current_page {
                row.add_css_class("selected");
            }
            let h = GtkBox::new(Orientation::Horizontal, 10);
            h.set_margin_top(8);
            h.set_margin_bottom(8);
            h.set_margin_start(10);
            h.set_margin_end(10);

            let ic = Label::new(Some(page_icon(*page)));
            ic.add_css_class("sidebar-icon");
            ic.set_size_request(20, -1);
            ic.set_xalign(0.0);
            h.append(&ic);

            let label = Label::new(Some(page.title()));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            h.append(&label);

            row.set_child(Some(&h));
            row.set_activatable(true);
            let s = sender.clone();
            let row_label = page.title().to_string();
            row.connect_activate(move |_| {
                tracing::info!(
                    target: "sidebar",
                    "row activated: {row_label}"
                );
                s.send_app(AppMsg::NavigateTo(*page));
            });
            list.append(&row);
        }

        // Belt-and-suspenders: also listen for row selection
        // changes. Some GTK4 configurations and themes swallow
        // the activate signal even with `set_activate_on_single_
        // click(true)`, but selection always fires on a normal
        // click in `Single` selection mode. This is a no-op when
        // the user just hovers or focuses (those don't change
        // the selection) so it doesn't spam NavigateTo.
        let sender_for_signal = sender.clone();
        list.connect_selected_rows_changed(move |sidebar| {
            let Some(row) = sidebar.selected_row() else {
                return;
            };
            let name = row.widget_name();
            let Some(page) = Page::from_key(name.as_str()) else {
                return;
            };
            tracing::info!(
                target: "sidebar",
                "row selected via signal: {}",
                page.title()
            );
            sender_for_signal.send_app(AppMsg::NavigateTo(page));
        });

        let widget = list.clone().upcast::<gtk::Widget>();
        (widget, list)
    }
}

fn page_icon(page: Page) -> &'static str {
    match page {
        Page::MyBose => "📶",
        Page::Overview => "🏠",
        Page::Audio => "🎚️",
        Page::Device => "⚙️",
        Page::Multipoint => "🔀",
        Page::Advanced => "🛠️",
    }
}

// ---------------------------------------------------------------------------
// HeaderBar (with the device switcher popover + theme toggle)
// ---------------------------------------------------------------------------

pub fn build_header_bar(model: &AppModel, sender: &flume::Sender<AppMsg>) -> adw::HeaderBar {
    let bar = adw::HeaderBar::new();
    bar.set_title_widget(Some(&adw::WindowTitle::new(
        "Bose Connect",
        model.current_page.title(),
    )));

    // Device switcher button (left).
    let switcher = gtk::MenuButton::new();
    switcher.add_css_class("device-switcher");
    let label = match &model.snapshot {
        Some(s) => format!("{}  ·  Connected", s.name),
        None => "No device selected".to_string(),
    };
    switcher.set_label(&label);
    let popover = gtk::Popover::new();
    let pop_box = GtkBox::new(Orientation::Vertical, 6);
    pop_box.set_margin_top(10);
    pop_box.set_margin_bottom(10);
    pop_box.set_margin_start(10);
    pop_box.set_margin_end(10);
    pop_box.set_size_request(240, -1);
    let pop_title = Label::new(Some("Switch device"));
    pop_title.add_css_class("row-desc");
    pop_title.set_xalign(0.0);
    pop_box.append(&pop_title);
    let pop_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    pop_box.append(&pop_sep);
    let mock_devices = vec![
        ("Bose QuietComfort 35 II", "AA:BB:CC:DD:EE:FF", true),
        ("Bose NC 700", "00:0C:8A:33:1B:42", false),
        ("Bose SoundLink Revolve", "BB:CC:DD:EE:FF:00", false),
    ];
    for (name, addr, active) in mock_devices {
        let row = Button::new();
        row.add_css_class("flat");
        let h = GtkBox::new(Orientation::Horizontal, 8);
        let meta = GtkBox::new(Orientation::Vertical, 1);
        meta.set_hexpand(true);
        let n = Label::new(Some(name));
        n.set_xalign(0.0);
        n.add_css_class("heading");
        meta.append(&n);
        let a = Label::new(Some(addr));
        a.set_xalign(0.0);
        a.add_css_class("row-desc");
        meta.append(&a);
        h.append(&meta);
        if active {
            let pill = Label::new(Some("●"));
            pill.add_css_class("success");
            pill.set_valign(gtk::Align::Center);
            h.append(&pill);
        }
        row.set_child(Some(&h));
        row.set_hexpand(true);
        pop_box.append(&row);
    }
    popover.set_child(Some(&pop_box));
    switcher.set_popover(Some(&popover));
    bar.pack_start(&switcher);

    // Theme toggle (right).
    let theme_btn = gtk::Button::with_label("🌓");
    theme_btn.set_tooltip_text(Some("Toggle light/dark"));
    theme_btn.add_css_class("theme-toggle");
    let style = adw::StyleManager::default();
    theme_btn.connect_clicked(move |_| {
        let next = match style.color_scheme() {
            adw::ColorScheme::ForceDark => adw::ColorScheme::ForceLight,
            _ => adw::ColorScheme::ForceDark,
        };
        style.set_color_scheme(next);
    });
    bar.pack_end(&theme_btn);

    // App menu (right).
    let menu = gtk::MenuButton::new();
    menu.set_icon_name("open-menu-symbolic");
    menu.set_menu_model(Some(&build_app_menu(sender)));
    bar.pack_end(&menu);

    bar
}

// ---------------------------------------------------------------------------
// App menu (Gio::Menu) — kept for the kebab in the header bar
// ---------------------------------------------------------------------------

pub fn build_app_menu(sender: &flume::Sender<AppMsg>) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    let section = gtk::gio::Menu::new();
    section.append(Some("Refresh device"), Some("app.refresh"));
    section.append(Some("About Bose Connect for Linux"), Some("app.about"));
    section.append(Some("Visit the project page"), Some("app.project"));
    section.append(Some("Quit"), Some("app.quit"));
    menu.append_section(None, &section);

    // We don't have a real `gtk::Application` here for `add_action`,
    // but the GUI wires these via the kebab's activate handler
    // through the `tx` sender. The kebab's `MenuButton` doesn't
    // route actions itself — it just shows the menu.
    let _ = sender;
    menu
}

// ---------------------------------------------------------------------------
// QuietModePill (kept for backward-compat re-export of the old
// headerbar element — the new layout uses the device switcher
// + theme toggle + app menu instead, but the symbol stays in
// place because some callers may still reference it.)
// ---------------------------------------------------------------------------

pub struct QuietModePill;

impl QuietModePill {
    #[allow(dead_code)]
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

// Keep `battery-display` happy even though we don't emit it as a
// class anymore — a few of the legacy CSS rules reference it.
#[allow(dead_code)]
fn _legacy_unused_marker() {
    // Suppress unused-import warnings if the gtk::Scale symbol is
    // ever pulled in by a future widget; intentionally empty.
}
