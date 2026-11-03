//! Widget sub-trees — pure functions that take `&AppModel` and a
//! message-sending handle and return a fully-formed widget.
//!
//! Follows GNOME Human Interface Guidelines (HIG) and Libadwaita standards:
//!   - Single unified, clamped view (AdwClamp max 720px)
//!   - Real Libadwaita widgets (AdwPreferencesGroup, AdwActionRow, AdwComboRow, AdwSwitchRow, AdwExpanderRow)
//!   - Standard system symbolic icons (no Unicode emojis)
//!   - Modals for renaming (with 31-byte limit & live counter) and forgetting devices
//!   - Serial number hide/reveal with selectable text

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
#[allow(unused_imports)]
use gtk::prelude::*;
use gtk::{glib, Box as GtkBox, Button, Image, Label, Orientation, StringList};

use crate::app::model::{AppModel, AppMsg, ConnectionState, Page};
use crate::t;

// ---------------------------------------------------------------------------
// Sender extension
// ---------------------------------------------------------------------------

pub trait SenderExt {
    fn send_app(&self, msg: AppMsg);
}

impl SenderExt for flume::Sender<AppMsg> {
    fn send_app(&self, msg: AppMsg) {
        let _ = self.send(msg);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn format_address(addr: bose_connect::BdAddr) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr.b[0], addr.b[1], addr.b[2], addr.b[3], addr.b[4], addr.b[5]
    )
}

fn build_sparkline(history: &[u8]) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("sparkline");
    row.set_homogeneous(true);
    row.set_size_request(-1, 24);

    let n = history.len().max(32);
    for i in 0..n {
        let bar = GtkBox::new(Orientation::Vertical, 0);
        bar.add_css_class("sparkline-bar");
        let h = match history.get(i) {
            Some(&v) => ((v as i32) * 20 / 100).max(3),
            None => 3,
        };
        bar.set_size_request(4, h);
        if i >= history.len() {
            bar.add_css_class("empty");
        }
        row.append(&bar);
    }
    row
}

pub fn segmented_control<F>(options: &[(&str, &str)], selected: &str, on_select: F) -> GtkBox
where
    F: Fn(&str) + 'static + Clone,
{
    let container = GtkBox::new(Orientation::Horizontal, 0);
    container.add_css_class("segmented");
    container.set_homogeneous(true);

    let buttons: Rc<RefCell<Vec<(String, gtk::ToggleButton)>>> = Rc::new(RefCell::new(Vec::new()));

    for (value, label) in options {
        let val_owned = value.to_string();
        let btn = gtk::ToggleButton::with_label(label);
        btn.add_css_class("segmented-button");
        if *value == selected {
            btn.set_active(true);
            btn.add_css_class("selected");
        }
        buttons.borrow_mut().push((val_owned.clone(), btn.clone()));
    }

    for (val, btn) in buttons.borrow().iter() {
        let val_clone = val.clone();
        let on_select = on_select.clone();
        let all_btns = buttons.clone();

        btn.connect_toggled(move |b| {
            if b.is_active() {
                for (other_val, other_btn) in all_btns.borrow().iter() {
                    if other_val != &val_clone {
                        other_btn.set_active(false);
                        other_btn.remove_css_class("selected");
                    }
                }
                b.add_css_class("selected");
                on_select(&val_clone);
            }
        });
        container.append(btn);
    }

    container
}

// ---------------------------------------------------------------------------
// Modals: Rename & Forget
// ---------------------------------------------------------------------------

pub fn open_rename_dialog(
    parent: &impl glib::object::IsA<gtk::Widget>,
    current_name: &str,
    sender: &flume::Sender<AppMsg>,
) {
    let root = parent.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    let dialog = adw::AlertDialog::new(Some(&t!("rename-title")), Some(&t!("rename-body")));
    dialog.add_response("cancel", &t!("dialog-cancel"));
    dialog.add_response("save", &t!("dialog-save"));
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

    let content_box = GtkBox::new(Orientation::Vertical, 6);
    content_box.set_margin_top(8);
    content_box.set_margin_bottom(8);
    content_box.set_margin_start(12);
    content_box.set_margin_end(12);

    let entry = gtk::Entry::new();
    entry.set_text(current_name);
    entry.set_max_length(31);
    content_box.append(&entry);

    let initial_len = current_name.len();
    let counter = Label::new(Some(&t!("rename-counter", length = initial_len)));
    counter.set_xalign(1.0);
    counter.add_css_class("row-desc");
    content_box.append(&counter);

    let d_clone = dialog.clone();
    let entry_weak = entry.downgrade();
    entry.connect_changed(move |e| {
        let text = e.text().to_string();
        let len = text.len();
        counter.set_text(&t!("rename-counter", length = len));
        if len == 0 || len > 31 {
            d_clone.set_response_enabled("save", false);
            counter.add_css_class("error");
        } else {
            d_clone.set_response_enabled("save", true);
            counter.remove_css_class("error");
        }
    });

    dialog.set_extra_child(Some(&content_box));

    let s = sender.clone();
    dialog.choose(
        root.as_ref(),
        None::<&gtk::gio::Cancellable>,
        move |choice| {
            if choice == "save" {
                if let Some(e) = entry_weak.upgrade() {
                    let new_name = e.text().to_string();
                    if !new_name.is_empty() && new_name.len() <= 31 {
                        s.send_app(AppMsg::SetName(new_name));
                    }
                }
            }
        },
    );
}

pub fn open_forget_dialog(
    parent: &impl glib::object::IsA<gtk::Widget>,
    name: &str,
    addr: bose_connect::BdAddr,
    sender: &flume::Sender<AppMsg>,
) {
    let root = parent.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    let addr_str = format_address(addr);
    let dialog = adw::AlertDialog::new(
        Some(&t!("forget-title")),
        Some(&t!("forget-body", name = name, address = addr_str.as_str())),
    );
    dialog.add_response("cancel", &t!("dialog-cancel"));
    dialog.add_response("delete", &t!("forget-confirm"));
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

    let s = sender.clone();
    dialog.choose(
        root.as_ref(),
        None::<&gtk::gio::Cancellable>,
        move |choice| {
            if choice == "delete" {
                s.send_app(AppMsg::PairedRemove(addr));
            }
        },
    );
}

// ---------------------------------------------------------------------------
// HeaderBar
// ---------------------------------------------------------------------------

pub struct HeaderBarWidgets {
    pub container: adw::HeaderBar,
    pub back_btn: Button,
    pub device_switcher: gtk::MenuButton,
    pub device_label: Label,
    pub window_title: adw::WindowTitle,
    /// Content of the device menu, refilled on every rebuild.
    pub device_menu: GtkBox,
    pub device_popover: gtk::Popover,
}

/// Icon for a paired device: a speaker or headphones.
fn device_icon(speaker: bool) -> &'static str {
    if speaker {
        "audio-speakers-symbolic"
    } else {
        "audio-headphones-symbolic"
    }
}

/// Text of the header's device button.
pub fn device_button_label(model: &AppModel) -> String {
    match (&model.snapshot, &model.connection) {
        (Some(s), _) => s.name.clone(),
        (None, ConnectionState::Connecting(_)) => t!("device-button-connecting"),
        _ => t!("device-button-none"),
    }
}

/// Fill the device menu: every paired Bose device, which ones BlueZ
/// has connected, a check on the one in use, and "manage devices".
pub fn fill_device_menu(
    menu: &GtkBox,
    popover: &gtk::Popover,
    model: &AppModel,
    sender: &flume::Sender<AppMsg>,
) {
    while let Some(child) = menu.first_child() {
        menu.remove(&child);
    }

    let title = Label::new(Some(&t!("device-menu-title")));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    menu.append(&title);
    menu.append(&gtk::Separator::new(Orientation::Horizontal));

    let active = model.active_address();
    let devices = model.host_devices.clone().unwrap_or_default();
    if devices.is_empty() {
        let empty = Label::new(Some(&if model.host_devices.is_none() {
            t!("bluez-down-title")
        } else {
            t!("device-menu-empty")
        }));
        empty.add_css_class("row-desc");
        empty.set_wrap(true);
        empty.set_xalign(0.0);
        menu.append(&empty);
    }
    for dev in devices {
        let btn = Button::new();
        btn.add_css_class("flat");
        let h = GtkBox::new(Orientation::Horizontal, 8);
        h.append(&Image::from_icon_name(device_icon(dev.is_speaker())));

        let meta = GtkBox::new(Orientation::Vertical, 1);
        meta.set_hexpand(true);
        let n_lbl = Label::new(Some(&dev.name));
        n_lbl.set_xalign(0.0);
        meta.append(&n_lbl);
        let status = if dev.connected {
            t!("device-menu-connected", address = dev.address.as_str())
        } else {
            t!("device-menu-disconnected", address = dev.address.as_str())
        };
        let a_lbl = Label::new(Some(&status));
        a_lbl.set_xalign(0.0);
        a_lbl.add_css_class("row-desc");
        meta.append(&a_lbl);
        h.append(&meta);

        let in_use = active
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(&dev.address));
        if in_use {
            let chk = Image::from_icon_name("object-select-symbolic");
            chk.add_css_class("success");
            h.append(&chk);
        }

        btn.set_child(Some(&h));
        let s = sender.clone();
        let p = popover.clone();
        let target_addr = dev.address.clone();
        btn.connect_clicked(move |_| {
            p.popdown();
            if !in_use {
                s.send_app(AppMsg::Connect(target_addr.clone()));
            }
        });
        menu.append(&btn);
    }

    menu.append(&gtk::Separator::new(Orientation::Horizontal));

    let manage_btn = Button::with_label(&t!("device-menu-manage"));
    manage_btn.add_css_class("flat");
    let s_manage = sender.clone();
    let p_manage = popover.clone();
    manage_btn.connect_clicked(move |_| {
        p_manage.popdown();
        s_manage.send_app(AppMsg::NavigateTo(Page::MyBose));
    });
    menu.append(&manage_btn);
}

pub fn build_header_bar(model: &AppModel, sender: &flume::Sender<AppMsg>) -> HeaderBarWidgets {
    let bar = adw::HeaderBar::new();
    bar.set_size_request(-1, 52);

    // Back button (shown on subpages like MyBose)
    let back_btn = Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some(&t!("header-back")));
    back_btn.set_valign(gtk::Align::Center);
    back_btn.set_visible(model.current_page == Page::MyBose);
    let s_back = sender.clone();
    back_btn.connect_clicked(move |_| {
        s_back.send_app(AppMsg::NavigateTo(Page::Overview));
    });
    bar.pack_start(&back_btn);

    // Device switcher button (shown on Overview)
    let switcher = gtk::MenuButton::new();
    switcher.add_css_class("device-switcher");
    switcher.set_valign(gtk::Align::Center);
    switcher.set_visible(model.current_page == Page::Overview);

    let switcher_box = GtkBox::new(Orientation::Horizontal, 6);
    let dev_icon = Image::from_icon_name("audio-headphones-symbolic");
    switcher_box.append(&dev_icon);

    let device_label = Label::new(Some(&device_button_label(model)));
    device_label.add_css_class("device-label");
    switcher_box.append(&device_label);

    let arrow = Image::from_icon_name("pan-down-symbolic");
    switcher_box.append(&arrow);
    switcher.set_child(Some(&switcher_box));

    // Popover for device switcher
    let popover = gtk::Popover::new();
    let pop_box = GtkBox::new(Orientation::Vertical, 6);
    pop_box.set_margin_top(8);
    pop_box.set_margin_bottom(8);
    pop_box.set_margin_start(8);
    pop_box.set_margin_end(8);
    pop_box.set_size_request(280, -1);
    fill_device_menu(&pop_box, &popover, model, sender);
    // Opening the menu re-reads BlueZ, so a device paired or
    // connected meanwhile shows up.
    let s_show = sender.clone();
    popover.connect_show(move |_| s_show.send_app(AppMsg::ReloadDevices));

    popover.set_child(Some(&pop_box));
    switcher.set_popover(Some(&popover));
    bar.pack_start(&switcher);

    // Window Title
    let window_title = adw::WindowTitle::new("Bose Connect", &model.current_page.subtitle());
    bar.set_title_widget(Some(&window_title));

    // Theme toggle button (right)
    let style = adw::StyleManager::default();
    let theme_icon_name = if style.is_dark() {
        "display-brightness-symbolic"
    } else {
        "weather-clear-night-symbolic"
    };
    let theme_btn = Button::from_icon_name(theme_icon_name);
    theme_btn.set_tooltip_text(Some(&t!("header-theme")));
    theme_btn.add_css_class("theme-toggle");
    theme_btn.set_valign(gtk::Align::Center);
    let btn_style = theme_btn.clone();
    theme_btn.connect_clicked(move |_| {
        let style = adw::StyleManager::default();
        let next = if style.is_dark() {
            btn_style.set_icon_name("weather-clear-night-symbolic");
            adw::ColorScheme::ForceLight
        } else {
            btn_style.set_icon_name("display-brightness-symbolic");
            adw::ColorScheme::ForceDark
        };
        style.set_color_scheme(next);
    });
    bar.pack_end(&theme_btn);

    // App menu kebab button (right)
    let app_menu_btn = gtk::MenuButton::new();
    app_menu_btn.set_icon_name("open-menu-symbolic");
    app_menu_btn.set_tooltip_text(Some(&t!("header-menu")));
    app_menu_btn.set_valign(gtk::Align::Center);
    app_menu_btn.set_menu_model(Some(&build_app_menu(sender)));
    bar.pack_end(&app_menu_btn);

    HeaderBarWidgets {
        container: bar,
        back_btn,
        device_switcher: switcher,
        device_label,
        window_title,
        device_menu: pop_box,
        device_popover: popover,
    }
}

pub fn build_app_menu(_sender: &flume::Sender<AppMsg>) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    let section = gtk::gio::Menu::new();
    section.append(Some(&t!("menu-refresh")), Some("win.refresh"));
    section.append(Some(&t!("menu-mybose")), Some("win.mybose"));
    section.append(Some(&t!("menu-about")), Some("win.about"));
    section.append(Some(&t!("menu-quit")), Some("win.quit"));
    menu.append_section(None, &section);
    menu
}

pub fn show_about_dialog(parent: &impl glib::object::IsA<gtk::Window>) {
    let dialog = adw::AboutDialog::builder()
        .application_name("Bose Connect")
        .developer_name("airvzxf")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(t!("about-comments", version = env!("CARGO_PKG_VERSION")))
        .website("https://github.com/airvzxf/bose-connect-app-linux")
        .issue_url("https://github.com/airvzxf/bose-connect-app-linux/issues")
        .license_type(gtk::License::Agpl30)
        .copyright("© 2024-2026 Bose Connect Contributors")
        .build();
    dialog.present(Some(parent.as_ref()));
}

// ---------------------------------------------------------------------------
// HeroCard
// ---------------------------------------------------------------------------

pub struct HeroCard;

impl HeroCard {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let card = GtkBox::new(Orientation::Horizontal, 16);
        card.add_css_class("hero-card");
        card.set_hexpand(true);
        // Do not inherit the icon's vexpand: without the settings
        // below it, the card would fill the whole window.
        card.set_vexpand(false);

        let (battery, name, address_str) = match (&model.snapshot, &model.connection) {
            (Some(s), _) => {
                let mac = s.device_bd_addr.unwrap_or(s.address);
                (s.battery as i32, s.name.clone(), format_address(mac))
            }
            (None, ConnectionState::Connecting(address)) => (
                -1,
                t!("hero-connecting", name = model.device_name(address)),
                address.clone(),
            ),
            (None, ConnectionState::Error(_)) => (-1, t!("hero-failed"), String::new()),
            _ => (-1, t!("hero-none"), String::new()),
        };

        // Left icon/avatar
        let icon_box = GtkBox::new(Orientation::Horizontal, 0);
        icon_box.add_css_class("hero-avatar");
        icon_box.set_size_request(56, 56);
        icon_box.set_valign(gtk::Align::Center);
        icon_box.set_halign(gtk::Align::Center);
        let speaker = model.active_address().is_some_and(|a| {
            model
                .host_devices
                .iter()
                .flatten()
                .any(|d| d.address.eq_ignore_ascii_case(&a) && d.is_speaker())
        });
        let ic_name = device_icon(speaker);
        let icon_img = Image::from_icon_name(ic_name);
        icon_img.set_pixel_size(28);
        icon_img.set_hexpand(true);
        icon_img.set_vexpand(true);
        icon_img.set_halign(gtk::Align::Center);
        icon_img.set_valign(gtk::Align::Center);
        icon_box.append(&icon_img);
        card.append(&icon_box);

        // Middle identity column
        let mid = GtkBox::new(Orientation::Vertical, 4);
        mid.set_hexpand(true);
        mid.set_valign(gtk::Align::Center);

        let title_row = GtkBox::new(Orientation::Horizontal, 6);
        let name_label = Label::new(None);
        name_label.set_markup(&format!(
            "<span weight='800' size='x-large'>{}</span>",
            glib::markup_escape_text(&name)
        ));
        name_label.set_xalign(0.0);
        title_row.append(&name_label);

        if model.snapshot.is_some() {
            let edit_btn = Button::from_icon_name("document-edit-symbolic");
            edit_btn.add_css_class("flat");
            edit_btn.add_css_class("circular");
            edit_btn.set_tooltip_text(Some(&t!("hero-rename")));
            let s_edit = sender.clone();
            let n_owned = name.clone();
            edit_btn.connect_clicked(move |b| {
                open_rename_dialog(b, &n_owned, &s_edit);
            });
            title_row.append(&edit_btn);
        }
        mid.append(&title_row);

        let sub_text = match (&model.snapshot, &model.connection) {
            (Some(s), _) if s.device_bd_addr.is_some() => {
                t!("hero-via-bd-addr", address = address_str.as_str())
            }
            (Some(_), _) => t!("hero-via-mac", address = address_str.as_str()),
            (None, ConnectionState::Connecting(_)) => address_str.clone(),
            (None, ConnectionState::Error(e)) => e.clone(),
            _ => t!("hero-pick"),
        };
        let sub_label = Label::new(Some(&sub_text));
        sub_label.set_xalign(0.0);
        sub_label.set_wrap(true);
        sub_label.add_css_class("row-desc");
        mid.append(&sub_label);

        let pills_box = GtkBox::new(Orientation::Horizontal, 6);
        let status_pill = Label::new(Some(&match model.connection {
            ConnectionState::Connected => t!("status-connected"),
            ConnectionState::Connecting(_) => t!("status-connecting"),
            _ => t!("status-disconnected"),
        }));
        status_pill.add_css_class("pill");
        if matches!(model.connection, ConnectionState::Connected) {
            status_pill.add_css_class("success");
        } else {
            status_pill.add_css_class("idle");
        }
        pills_box.append(&status_pill);
        mid.append(&pills_box);

        card.append(&mid);
        if model.snapshot.is_none() {
            return card.upcast::<gtk::Widget>();
        }

        // Right battery column
        let right = GtkBox::new(Orientation::Vertical, 6);
        right.set_valign(gtk::Align::Center);
        right.set_size_request(140, -1);

        let bat_row = GtkBox::new(Orientation::Horizontal, 6);
        bat_row.set_halign(gtk::Align::End);
        let (bat_icon_name, bat_color_class) = if battery >= 70 {
            ("battery-full-symbolic", "success")
        } else if battery >= 35 {
            ("battery-good-symbolic", "warning")
        } else if battery >= 15 {
            ("battery-low-symbolic", "error")
        } else if battery >= 5 {
            ("battery-caution-symbolic", "error")
        } else {
            ("battery-empty-symbolic", "error")
        };
        let bat_icon = Image::from_icon_name(bat_icon_name);
        bat_icon.add_css_class("battery-icon");
        bat_icon.add_css_class(bat_color_class);
        bat_row.append(&bat_icon);

        let pct_str = if battery < 0 {
            "—".to_string()
        } else {
            format!("{battery}%")
        };
        let bat_label = Label::new(None);
        bat_label.set_markup(&format!("<span size='large'>{pct_str}</span>"));
        bat_row.append(&bat_label);
        right.append(&bat_row);

        let spark = build_sparkline(&model.history);
        right.append(&spark);

        let tip = Label::new(Some(&t!("hero-history")));
        tip.add_css_class("row-desc");
        tip.set_xalign(1.0);
        right.append(&tip);

        card.append(&right);

        card.upcast::<gtk::Widget>()
    }
}

/// Row that shows how BlueZ sees the audio link to this computer,
/// with the reconnect advice when it is connected without audio.
fn bluez_audio_row(state: bose_connect::AudioState) -> adw::ActionRow {
    use bose_connect::AudioState;
    let (title, icon) = match state {
        AudioState::Unknown => (t!("audio-unknown"), "bluetooth-disabled-symbolic"),
        AudioState::Disconnected => (t!("audio-disconnected"), "bluetooth-disabled-symbolic"),
        AudioState::ConnectedWithoutAudio => (t!("audio-without-audio"), "dialog-warning-symbolic"),
        AudioState::Idle => (t!("audio-idle"), "audio-speakers-symbolic"),
        AudioState::Playing => (t!("audio-playing"), "audio-speakers-symbolic"),
    };
    let row = adw::ActionRow::new();
    row.set_title(&t!("audio-row-title"));
    row.set_subtitle(&if state.needs_reconnect_hint() {
        t!("audio-reconnect-hint")
    } else {
        title
    });
    row.set_subtitle_lines(0);
    let image = Image::from_icon_name(icon);
    if state.needs_reconnect_hint() {
        image.add_css_class("warning");
    }
    row.add_prefix(&image);
    row
}

// ---------------------------------------------------------------------------
// OverviewPage (Main unified control center)
// ---------------------------------------------------------------------------

pub struct OverviewPage;

impl OverviewPage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 20);
        page.set_hexpand(true);
        page.set_vexpand(true);

        if model.snapshot.is_none() && model.no_paired_devices() {
            page.append(&no_devices_status_page(sender));
            return page.upcast::<gtk::Widget>();
        }

        // 1. Hero Card
        page.append(&HeroCard::render(model, sender));

        // Not connected: the same window, empty, with the devices
        // the user can connect to.
        if model.snapshot.is_none() {
            page.append(&connect_group(model, sender));
            return page.upcast::<gtk::Widget>();
        }

        // 2. Audio y Reproducción (Media & Volume)
        if let Some(snapshot) = &model.snapshot {
            let audio_group = adw::PreferencesGroup::new();
            audio_group.set_title(&t!("group-audio"));

            // Fila 0: el enlace de audio según BlueZ.
            if let Some(state) = snapshot.audio {
                audio_group.add(&bluez_audio_row(state));
            }

            // Fila 1: Control multimedia
            let media_row = adw::ActionRow::new();
            media_row.set_title(&t!("media-title"));
            media_row.set_subtitle(&t!("media-subtitle"));
            media_row.add_prefix(&Image::from_icon_name("media-playback-start-symbolic"));

            let btn_box = GtkBox::new(Orientation::Horizontal, 0);
            btn_box.add_css_class("linked");

            let prev_btn = Button::builder()
                .icon_name("media-skip-backward-symbolic")
                .tooltip_text(t!("media-previous"))
                .build();
            let s_prev = sender.clone();
            prev_btn.connect_clicked(move |_| {
                s_prev.send_app(AppMsg::SendMediaKey(bose_connect::MediaKey::Previous));
            });
            btn_box.append(&prev_btn);

            let pause_btn = Button::builder()
                .icon_name("media-playback-start-symbolic")
                .tooltip_text(t!("media-play"))
                .build();
            let is_playing = Rc::new(Cell::new(false));
            let is_playing_clone = is_playing.clone();
            let pause_btn_clone = pause_btn.clone();
            let s_pause = sender.clone();
            pause_btn.connect_clicked(move |_| {
                let playing = !is_playing_clone.get();
                is_playing_clone.set(playing);
                if playing {
                    pause_btn_clone.set_icon_name("media-playback-pause-symbolic");
                    pause_btn_clone.set_tooltip_text(Some(&t!("media-pause")));
                } else {
                    pause_btn_clone.set_icon_name("media-playback-start-symbolic");
                    pause_btn_clone.set_tooltip_text(Some(&t!("media-play")));
                }
                s_pause.send_app(AppMsg::SendMediaKey(bose_connect::MediaKey::Pause));
            });
            btn_box.append(&pause_btn);

            let next_btn = Button::builder()
                .icon_name("media-skip-forward-symbolic")
                .tooltip_text(t!("media-next"))
                .build();
            let s_next = sender.clone();
            next_btn.connect_clicked(move |_| {
                s_next.send_app(AppMsg::SendMediaKey(bose_connect::MediaKey::Next));
            });
            btn_box.append(&next_btn);

            media_row.add_suffix(&btn_box);
            audio_group.add(&media_row);

            // Fila 2: Volumen del auricular (0 a 75)
            if let Some(vol) = snapshot.volume {
                let vol_row = adw::ActionRow::new();
                vol_row.set_title(&t!("volume-title"));
                vol_row.set_subtitle(&t!("volume-subtitle"));

                let vol_icon_name = if vol == 0 {
                    "audio-volume-muted-symbolic"
                } else if vol < 25 {
                    "audio-volume-low-symbolic"
                } else if vol < 50 {
                    "audio-volume-medium-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                };
                let vol_img = Image::from_icon_name(vol_icon_name);
                vol_row.add_prefix(&vol_img);

                let vol_box = GtkBox::new(Orientation::Horizontal, 12);
                let scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 75.0, 1.0);
                scale.set_value(vol as f64);
                scale.set_size_request(180, -1);
                scale.set_draw_value(false);

                let pct = (vol as f64 / 75.0 * 100.0).round() as u32;
                let vol_lbl = Label::new(Some(&t!("volume-value", level = vol, percent = pct)));
                vol_lbl.set_size_request(110, -1);
                vol_lbl.set_xalign(1.0);
                vol_lbl.add_css_class("row-value");

                let s_vol = sender.clone();
                let vol_img_clone = vol_img.clone();
                let vol_lbl_clone = vol_lbl.clone();
                scale.connect_value_changed(move |s| {
                    let v = s.value().clamp(0.0, 75.0).round() as u8;
                    let p = (v as f64 / 75.0 * 100.0).round() as u32;
                    vol_lbl_clone.set_text(&t!("volume-value", level = v, percent = p));
                    let ic = if v == 0 {
                        "audio-volume-muted-symbolic"
                    } else if v < 25 {
                        "audio-volume-low-symbolic"
                    } else if v < 50 {
                        "audio-volume-medium-symbolic"
                    } else {
                        "audio-volume-high-symbolic"
                    };
                    vol_img_clone.set_icon_name(Some(ic));
                    s_vol.send_app(AppMsg::SetVolume(v));
                });

                vol_box.append(&scale);
                vol_box.append(&vol_lbl);
                vol_row.add_suffix(&vol_box);
                audio_group.add(&vol_row);
            }

            page.append(&audio_group);
        }

        let cap = model.capabilities();

        // 3. Control del Auricular (Headset controls)
        if cap.noise_cancelling || cap.self_voice {
            let headset_group = adw::PreferencesGroup::new();
            headset_group.set_title(&t!("group-headset"));

            if cap.noise_cancelling {
                let anc_row = adw::ActionRow::new();
                anc_row.set_title(&t!("anc-title"));
                anc_row.set_subtitle(&t!("anc-subtitle"));
                anc_row.add_prefix(&Image::from_icon_name("audio-volume-muted-symbolic"));

                let current_nc = match model.snapshot.as_ref().map(|s| s.status.level) {
                    Some(bose_connect::NoiseCancelling::High) => "high",
                    Some(bose_connect::NoiseCancelling::Low) => "low",
                    _ => "off",
                };
                let s_nc = sender.clone();
                let seg = segmented_control(
                    &[
                        ("off", &t!("level-off")),
                        ("low", &t!("level-low")),
                        ("high", &t!("level-high")),
                    ],
                    current_nc,
                    move |v| match v {
                        "off" => s_nc.send_app(AppMsg::SetNoiseCancelling(
                            bose_connect::NoiseCancelling::Off,
                        )),
                        "low" => s_nc.send_app(AppMsg::SetNoiseCancelling(
                            bose_connect::NoiseCancelling::Low,
                        )),
                        "high" => s_nc.send_app(AppMsg::SetNoiseCancelling(
                            bose_connect::NoiseCancelling::High,
                        )),
                        _ => {}
                    },
                );
                anc_row.add_suffix(&seg);
                headset_group.add(&anc_row);
            }

            if cap.self_voice {
                let sv_row = adw::ComboRow::new();
                sv_row.set_title(&t!("self-voice-title"));
                sv_row.set_subtitle(&t!("self-voice-subtitle"));
                sv_row.add_prefix(&Image::from_icon_name("audio-input-microphone-symbolic"));
                let sv_strings = StringList::new(&[
                    &t!("level-off"),
                    &t!("level-low"),
                    &t!("level-medium"),
                    &t!("level-high"),
                ]);
                sv_row.set_model(Some(&sv_strings));

                // Default to medium
                sv_row.set_selected(2);

                let s_sv = sender.clone();
                sv_row.connect_selected_notify(move |r| {
                    let level = match r.selected() {
                        0 => bose_connect::SelfVoice::Off,
                        1 => bose_connect::SelfVoice::Low,
                        2 => bose_connect::SelfVoice::Medium,
                        3 => bose_connect::SelfVoice::High,
                        _ => bose_connect::SelfVoice::Off,
                    };
                    s_sv.send_app(AppMsg::SetSelfVoice(level));
                });
                headset_group.add(&sv_row);
            }

            page.append(&headset_group);
        }

        // 4. Voz e Indicaciones
        let voice_group = adw::PreferencesGroup::new();
        voice_group.set_title(&t!("group-voice"));

        let voice_on = matches!(
            model.snapshot.as_ref(),
            Some(s) if s.status.language & bose_connect::VP_ENABLE_BIT != 0
        );
        let vp_row = adw::SwitchRow::new();
        vp_row.set_title(&t!("voice-prompts-title"));
        vp_row.set_subtitle(&t!("voice-prompts-subtitle"));
        vp_row.add_prefix(&Image::from_icon_name("audio-volume-high-symbolic"));
        vp_row.set_active(voice_on);
        let s_vp = sender.clone();
        vp_row.connect_active_notify(move |r| {
            s_vp.send_app(AppMsg::SetVoicePrompts(r.is_active()));
        });
        voice_group.add(&vp_row);

        let lang_row = adw::ComboRow::new();
        lang_row.set_title(&t!("prompt-language-title"));
        lang_row.set_subtitle(&t!("prompt-language-subtitle"));
        lang_row.add_prefix(&Image::from_icon_name(
            "preferences-desktop-locale-symbolic",
        ));
        // Native names, in the order of `prompt_languages()`.
        let languages = crate::transport::prompt_languages();
        let mut names: Vec<String> = languages
            .iter()
            .map(|l| crate::transport::prompt_language_label(*l).to_owned())
            .collect();
        // Only the low 5 bits are the language: a QC Ultra also sets
        // bits 7 and 6, which a plain mask used to read as English.
        let byte = model.snapshot.as_ref().map(|s| s.status.language);
        let current = byte.and_then(bose_connect::PromptLanguage::from_status_byte);
        let selected = match (byte, current) {
            (_, Some(lang)) => languages.iter().position(|l| *l == lang),
            (Some(byte), None) => {
                names.push(t!(
                    "prompt-language-unknown",
                    code = format!("0x{byte:02x}")
                ));
                Some(names.len() - 1)
            }
            (None, None) => None,
        };
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        lang_row.set_model(Some(&StringList::new(&name_refs)));
        if let Some(pos) = selected {
            lang_row.set_selected(pos as u32);
        }
        // Same rule as the CLI: no language write where its payload
        // layout has not been tested.
        if model
            .snapshot
            .as_ref()
            .is_some_and(|s| !bose_connect::has_legacy_settings(s.device_id))
        {
            lang_row.set_sensitive(false);
            lang_row.set_subtitle(&t!("prompt-language-read-only"));
        }

        let s_lang = sender.clone();
        lang_row.connect_selected_notify(move |r| {
            // The "unknown" entry, if any, is past the languages.
            if let Some(&language) = languages.get(r.selected() as usize) {
                s_lang.send_app(AppMsg::SetLanguage {
                    language,
                    voice_prompts: true,
                });
            }
        });
        voice_group.add(&lang_row);
        page.append(&voice_group);

        // 5. Energía y Batería
        let power_group = adw::PreferencesGroup::new();
        power_group.set_title(&t!("group-power"));

        let ao_row = adw::ComboRow::new();
        ao_row.set_title(&t!("auto-off-title"));
        ao_row.set_subtitle(&t!("auto-off-subtitle"));
        ao_row.add_prefix(&Image::from_icon_name("clock-symbolic"));
        let mut ao_choices = vec![t!("auto-off-never")];
        for minutes in [5, 20, 40, 60, 180] {
            ao_choices.push(t!("auto-off-minutes", minutes = minutes));
        }
        let ao_choices: Vec<&str> = ao_choices.iter().map(String::as_str).collect();
        let ao_model = StringList::new(&ao_choices);
        ao_row.set_model(Some(&ao_model));

        let current_ao = match model.snapshot.as_ref().and_then(|s| s.status.minutes) {
            Some(0) => 0,
            Some(5) => 1,
            Some(20) => 2,
            Some(40) => 3,
            Some(60) => 4,
            Some(180) => 5,
            _ => 0,
        };
        ao_row.set_selected(current_ao);

        let s_ao = sender.clone();
        ao_row.connect_selected_notify(move |r| {
            let mins = match r.selected() {
                0 => bose_connect::AutoOff::Never,
                1 => bose_connect::AutoOff::Min5,
                2 => bose_connect::AutoOff::Min20,
                3 => bose_connect::AutoOff::Min40,
                4 => bose_connect::AutoOff::Min60,
                5 => bose_connect::AutoOff::Min180,
                _ => bose_connect::AutoOff::Never,
            };
            s_ao.send_app(AppMsg::SetAutoOff(mins));
        });
        power_group.add(&ao_row);
        page.append(&power_group);

        // 6. Dispositivos Vinculados al Bose (Multipunto)
        let mp_group = adw::PreferencesGroup::new();
        mp_group.set_title(&t!("group-multipoint"));

        let pair_row = adw::SwitchRow::new();
        pair_row.set_title(&t!("pairing-title"));
        pair_row.set_subtitle(&t!("pairing-subtitle"));
        pair_row.add_prefix(&Image::from_icon_name(
            "preferences-system-bluetooth-symbolic",
        ));
        let s_pair = sender.clone();
        pair_row.connect_active_notify(move |r| {
            s_pair.send_app(AppMsg::SetPairing(r.is_active()));
        });
        mp_group.add(&pair_row);

        let devices = model
            .snapshot
            .as_ref()
            .map(|s| s.devices.clone())
            .unwrap_or_default();

        if devices.is_empty() {
            let row = adw::ActionRow::new();
            row.set_title(&t!("sources-empty"));
            row.set_subtitle(&t!("sources-empty-hint"));
            row.add_prefix(&Image::from_icon_name("dialog-information-symbolic"));
            mp_group.add(&row);
        } else {
            let active_addr = model.snapshot.as_ref().and_then(|s| s.active_device);
            for dev in devices {
                let name = match std::str::from_utf8(&dev.name[..dev.name_len]) {
                    Ok(s) => s.to_string(),
                    Err(_) => t!("source-unnamed", address = format_address(dev.address)),
                };
                let row = adw::ActionRow::new();
                row.set_title(&name);
                row.set_subtitle(&format_address(dev.address));

                let icon = if name.contains("Bose") || name.contains("SLC") {
                    "audio-speakers-symbolic"
                } else if name.to_lowercase().contains("pixel")
                    || name.to_lowercase().contains("phone")
                    || name.to_lowercase().contains("edge")
                {
                    "phone-symbolic"
                } else {
                    "computer-symbolic"
                };
                row.add_prefix(&Image::from_icon_name(icon));

                match dev.status {
                    bose_connect::DeviceStatus::This => {
                        let pill = Label::new(Some(&t!("source-this")));
                        pill.add_css_class("pill");
                        pill.add_css_class("success");
                        row.add_suffix(&pill);

                        if active_addr == Some(dev.address) {
                            let act_pill = Label::new(Some(&t!("source-active-audio")));
                            act_pill.add_css_class("pill");
                            act_pill.add_css_class("accent");
                            row.add_suffix(&act_pill);
                        }

                        let dc_btn = Button::with_label(&t!("action-disconnect"));
                        dc_btn.add_css_class("flat");
                        let s = sender.clone();
                        dc_btn.connect_clicked(move |_| {
                            s.send_app(AppMsg::PairedDisconnect(dev.address));
                        });
                        row.add_suffix(&dc_btn);
                    }
                    bose_connect::DeviceStatus::Connected => {
                        let pill = Label::new(Some(&t!("device-connected")));
                        pill.add_css_class("pill");
                        pill.add_css_class("success");
                        row.add_suffix(&pill);

                        if active_addr == Some(dev.address) {
                            let act_pill = Label::new(Some(&t!("source-active-audio")));
                            act_pill.add_css_class("pill");
                            act_pill.add_css_class("accent");
                            row.add_suffix(&act_pill);
                        }

                        let dc_btn = Button::with_label(&t!("action-disconnect"));
                        dc_btn.add_css_class("flat");
                        let s = sender.clone();
                        dc_btn.connect_clicked(move |_| {
                            s.send_app(AppMsg::PairedDisconnect(dev.address));
                        });
                        row.add_suffix(&dc_btn);

                        let forget_btn = Button::from_icon_name("user-trash-symbolic");
                        forget_btn.add_css_class("flat");
                        forget_btn.set_tooltip_text(Some(&t!("action-forget")));
                        let s_del = sender.clone();
                        let n_owned = name.clone();
                        forget_btn.connect_clicked(move |b| {
                            open_forget_dialog(b, &n_owned, dev.address, &s_del);
                        });
                        row.add_suffix(&forget_btn);
                    }
                    bose_connect::DeviceStatus::Disconnected => {
                        let cn_btn = Button::with_label(&t!("action-connect"));
                        cn_btn.add_css_class("suggested-action");
                        let s = sender.clone();
                        cn_btn.connect_clicked(move |_| {
                            s.send_app(AppMsg::PairedConnect(dev.address));
                        });
                        row.add_suffix(&cn_btn);

                        let forget_btn = Button::from_icon_name("user-trash-symbolic");
                        forget_btn.add_css_class("flat");
                        forget_btn.set_tooltip_text(Some(&t!("action-forget")));
                        let s_del = sender.clone();
                        let n_owned = name.clone();
                        forget_btn.connect_clicked(move |b| {
                            open_forget_dialog(b, &n_owned, dev.address, &s_del);
                        });
                        row.add_suffix(&forget_btn);
                    }
                }

                mp_group.add(&row);
            }
        }
        page.append(&mp_group);

        // 7. Información del Dispositivo
        let info_group = adw::PreferencesGroup::new();
        info_group.set_title(&t!("group-info"));

        let (device_id, serial, firmware, bd_addr) = match &model.snapshot {
            Some(s) => (
                format!("0x{:04X}", s.device_id),
                s.serial.clone(),
                format!("v{}", s.firmware),
                s.device_bd_addr
                    .map(format_address)
                    .unwrap_or_else(|| "—".to_string()),
            ),
            None => (
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
            ),
        };

        let id_row = adw::ActionRow::new();
        id_row.set_title(&t!("info-device-id"));
        id_row.add_prefix(&Image::from_icon_name("dialog-information-symbolic"));
        let id_lbl = Label::new(Some(&device_id));
        id_lbl.set_selectable(true);
        id_lbl.add_css_class("row-value");
        id_row.add_suffix(&id_lbl);
        info_group.add(&id_row);

        let mac_row = adw::ActionRow::new();
        mac_row.set_title(&t!("info-bd-addr"));
        mac_row.set_subtitle(&t!("info-bd-addr-subtitle"));
        mac_row.add_prefix(&Image::from_icon_name(
            "preferences-system-bluetooth-symbolic",
        ));
        let mac_lbl = Label::new(Some(&bd_addr));
        mac_lbl.set_selectable(true);
        mac_lbl.add_css_class("mono");
        mac_lbl.add_css_class("row-value");
        mac_row.add_suffix(&mac_lbl);
        info_group.add(&mac_row);

        let fw_row = adw::ActionRow::new();
        fw_row.set_title(&t!("info-firmware"));
        fw_row.add_prefix(&Image::from_icon_name("system-software-update-symbolic"));
        let fw_lbl = Label::new(Some(&firmware));
        fw_lbl.set_selectable(true);
        fw_lbl.add_css_class("row-value");
        fw_row.add_suffix(&fw_lbl);
        info_group.add(&fw_row);

        let sn_row = adw::ActionRow::new();
        sn_row.set_title(&t!("info-serial"));
        sn_row.add_prefix(&Image::from_icon_name("emblem-system-symbolic"));

        let sn_box = GtkBox::new(Orientation::Horizontal, 8);
        let sn_lbl = Label::new(Some("•••••••••••••••••"));
        sn_lbl.set_selectable(true);
        sn_lbl.add_css_class("mono");
        sn_box.append(&sn_lbl);

        let reveal_btn = Button::from_icon_name("view-reveal-symbolic");
        reveal_btn.add_css_class("flat");
        reveal_btn.set_tooltip_text(Some(&t!("info-serial-show")));
        let is_revealed = Rc::new(Cell::new(false));
        {
            let is_revealed = is_revealed.clone();
            let sn_lbl = sn_lbl.clone();
            let btn_for_click = reveal_btn.clone();
            let serial_owned = serial.clone();
            reveal_btn.connect_clicked(move |_| {
                let rev = !is_revealed.get();
                is_revealed.set(rev);
                if rev {
                    sn_lbl.set_text(&serial_owned);
                    btn_for_click.set_icon_name("view-conceal-symbolic");
                    btn_for_click.set_tooltip_text(Some(&t!("info-serial-hide")));
                } else {
                    sn_lbl.set_text("•••••••••••••••••");
                    btn_for_click.set_icon_name("view-reveal-symbolic");
                    btn_for_click.set_tooltip_text(Some(&t!("info-serial-show")));
                }
            });
        }
        sn_box.append(&reveal_btn);
        sn_row.add_suffix(&sn_box);
        info_group.add(&sn_row);

        page.append(&info_group);

        // 8. Avanzado
        let adv_group = adw::PreferencesGroup::new();
        adv_group.set_title(&t!("group-advanced"));

        let ver_row = adw::ActionRow::new();
        ver_row.set_title(&t!("info-versions"));
        ver_row.add_prefix(&Image::from_icon_name("system-software-update-symbolic"));
        let ver_lbl = Label::new(Some(&format!(
            "bose-connect-gui v{0}  ·  bose-connect v{0}",
            env!("CARGO_PKG_VERSION")
        )));
        ver_lbl.set_selectable(true);
        ver_lbl.add_css_class("row-desc");
        ver_row.add_suffix(&ver_lbl);
        adv_group.add(&ver_row);

        page.append(&adv_group);

        page.upcast::<gtk::Widget>()
    }
}

// ---------------------------------------------------------------------------
// Empty states
// ---------------------------------------------------------------------------

/// The window when no Bose device is paired with this computer.
fn no_devices_status_page(sender: &flume::Sender<AppMsg>) -> gtk::Widget {
    let status = adw::StatusPage::new();
    status.set_icon_name(Some("audio-headphones-symbolic"));
    status.set_title(&t!("no-devices-title"));
    status.set_description(Some(&format!(
        "{}\n\n{}",
        t!("no-devices-body"),
        t!("pairing-steps")
    )));
    status.set_vexpand(true);
    let again = Button::with_label(&t!("no-devices-again"));
    again.add_css_class("pill");
    again.add_css_class("suggested-action");
    again.set_halign(gtk::Align::Center);
    let s = sender.clone();
    again.connect_clicked(move |_| s.send_app(AppMsg::ReloadDevices));
    status.set_child(Some(&again));
    status.upcast::<gtk::Widget>()
}

/// One row per paired Bose device; the one in use gets "Disconnect",
/// the others "Use" (disabled while a connection is being opened).
fn paired_device_rows(
    group: &adw::PreferencesGroup,
    model: &AppModel,
    sender: &flume::Sender<AppMsg>,
) {
    let Some(devices) = &model.host_devices else {
        let row = adw::ActionRow::new();
        row.set_title(&t!("bluez-down-title"));
        row.set_subtitle(&t!("bluez-down-subtitle"));
        row.add_prefix(&Image::from_icon_name("dialog-warning-symbolic"));
        group.add(&row);
        return;
    };
    let active = model.active_address();
    let connecting = matches!(model.connection, ConnectionState::Connecting(_));
    for dev in devices {
        let row = adw::ActionRow::new();
        row.set_title(&dev.name);
        row.set_subtitle(&dev.address);
        row.add_prefix(&Image::from_icon_name(device_icon(dev.is_speaker())));

        let pill = Label::new(Some(&if dev.connected {
            t!("device-connected")
        } else {
            t!("device-not-connected")
        }));
        pill.add_css_class("pill");
        pill.add_css_class(if dev.connected { "success" } else { "muted" });
        pill.set_valign(gtk::Align::Center);
        row.add_suffix(&pill);

        let in_use = active
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(&dev.address));
        if in_use && !connecting {
            let used = Label::new(Some(&t!("device-in-use")));
            used.add_css_class("pill");
            used.add_css_class("accent");
            used.set_valign(gtk::Align::Center);
            row.add_suffix(&used);
        }
        // "Use" (control it from here), not "Connect": the pill
        // already says whether BlueZ has it connected.
        let btn = if in_use && !connecting {
            let b = Button::with_label(&t!("action-disconnect"));
            let s = sender.clone();
            b.connect_clicked(move |_| s.send_app(AppMsg::Disconnect));
            b
        } else {
            let b = Button::with_label(&if in_use {
                t!("action-connecting")
            } else {
                t!("action-use")
            });
            b.add_css_class("suggested-action");
            b.set_sensitive(!connecting);
            let s = sender.clone();
            let address = dev.address.clone();
            b.connect_clicked(move |_| s.send_app(AppMsg::Connect(address.clone())));
            b
        };
        btn.set_valign(gtk::Align::Center);
        row.add_suffix(&btn);
        group.add(&row);
    }
}

/// "Connect to a device": shown instead of the settings when the GUI
/// is not connected.
fn connect_group(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
    let group = adw::PreferencesGroup::new();
    group.set_title(&t!("connect-group-title"));
    group.set_description(Some(&t!("connect-group-description")));
    paired_device_rows(&group, model, sender);
    group.upcast::<gtk::Widget>()
}

// ---------------------------------------------------------------------------
// MyBosePage (the Bose devices paired with this computer)
// ---------------------------------------------------------------------------

pub struct MyBosePage;

impl MyBosePage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 20);
        page.set_hexpand(true);
        page.set_vexpand(true);

        let devices_group = adw::PreferencesGroup::new();
        devices_group.set_title(&t!("mybose-group-title"));
        let reload = Button::from_icon_name("view-refresh-symbolic");
        reload.add_css_class("flat");
        reload.set_tooltip_text(Some(&t!("mybose-reload")));
        let s_reload = sender.clone();
        reload.connect_clicked(move |_| s_reload.send_app(AppMsg::ReloadDevices));
        devices_group.set_header_suffix(Some(&reload));
        if model.no_paired_devices() {
            let row = adw::ActionRow::new();
            row.set_title(&t!("no-devices-title"));
            row.add_prefix(&Image::from_icon_name("dialog-information-symbolic"));
            devices_group.add(&row);
        } else {
            paired_device_rows(&devices_group, model, sender);
        }
        page.append(&devices_group);

        let pair_group = adw::PreferencesGroup::new();
        pair_group.set_title(&t!("mybose-pair-title"));
        let steps = Label::new(Some(&t!("pairing-steps")));
        steps.set_wrap(true);
        steps.set_xalign(0.0);
        steps.add_css_class("row-desc");
        pair_group.add(&steps);
        page.append(&pair_group);

        page.upcast::<gtk::Widget>()
    }
}
