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
use crate::i18n;

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
    let dialog = adw::AlertDialog::new(
        Some("Renombrar Dispositivo"),
        Some("Introduce el nuevo nombre Bluetooth para el dispositivo (máximo 31 bytes):"),
    );
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("save", "Guardar");
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
    let counter = Label::new(Some(&format!("{initial_len} / 31 bytes")));
    counter.set_xalign(1.0);
    counter.add_css_class("row-desc");
    content_box.append(&counter);

    let d_clone = dialog.clone();
    let entry_weak = entry.downgrade();
    entry.connect_changed(move |e| {
        let text = e.text().to_string();
        let len = text.len();
        counter.set_text(&format!("{len} / 31 bytes"));
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
        Some("¿Olvidar dispositivo?"),
        Some(&format!(
            "¿Deseas eliminar \"{name}\" ({addr_str}) de la memoria del auricular? Tendrás que volver a emparejarlo manualmente."
        )),
    );
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("delete", "Olvidar");
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
}

pub fn build_header_bar(model: &AppModel, sender: &flume::Sender<AppMsg>) -> HeaderBarWidgets {
    let bar = adw::HeaderBar::new();
    bar.set_size_request(-1, 52);

    // Back button (shown on subpages like MyBose)
    let back_btn = Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some("Volver al dispositivo"));
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

    let initial_name = model
        .snapshot
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Desconectado".to_string());
    let device_label = Label::new(Some(&initial_name));
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
    pop_box.set_size_request(260, -1);

    let pop_title = Label::new(Some("Dispositivos guardados"));
    pop_title.add_css_class("heading");
    pop_title.set_xalign(0.0);
    pop_box.append(&pop_title);

    pop_box.append(&gtk::Separator::new(Orientation::Horizontal));

    let known_devices = [
        ("Bose SLC II White 🐺", "04:52:C7:BA:68:0D"),
        ("Bose SLC II Black 🐺", "2C:41:A1:0B:7C:82"),
        ("Bose QuietComfort 35 II", "AA:BB:CC:DD:EE:FF"),
    ];

    for (dname, daddr) in known_devices {
        let btn = Button::new();
        btn.add_css_class("flat");
        let h = GtkBox::new(Orientation::Horizontal, 8);
        let ic = Image::from_icon_name(if dname.contains("SLC") {
            "audio-speakers-symbolic"
        } else {
            "audio-headphones-symbolic"
        });
        h.append(&ic);

        let meta = GtkBox::new(Orientation::Vertical, 1);
        meta.set_hexpand(true);
        let n_lbl = Label::new(Some(dname));
        n_lbl.set_xalign(0.0);
        meta.append(&n_lbl);
        let a_lbl = Label::new(Some(daddr));
        a_lbl.set_xalign(0.0);
        a_lbl.add_css_class("row-desc");
        meta.append(&a_lbl);
        h.append(&meta);

        if dname == initial_name {
            let chk = Image::from_icon_name("object-select-symbolic");
            chk.add_css_class("success");
            h.append(&chk);
        }

        btn.set_child(Some(&h));
        let s = sender.clone();
        let p = popover.clone();
        let target_addr = daddr.to_string();
        btn.connect_clicked(move |_| {
            p.popdown();
            s.send_app(AppMsg::Connect(target_addr.clone()));
        });
        pop_box.append(&btn);
    }

    pop_box.append(&gtk::Separator::new(Orientation::Horizontal));

    let manage_btn = Button::with_label("Gestionar dispositivos Bose…");
    manage_btn.add_css_class("flat");
    let s_manage = sender.clone();
    let p_manage = popover.clone();
    manage_btn.connect_clicked(move |_| {
        p_manage.popdown();
        s_manage.send_app(AppMsg::NavigateTo(Page::MyBose));
    });
    pop_box.append(&manage_btn);

    popover.set_child(Some(&pop_box));
    switcher.set_popover(Some(&popover));
    bar.pack_start(&switcher);

    // Window Title
    let window_title = adw::WindowTitle::new("Bose Connect", model.current_page.subtitle());
    bar.set_title_widget(Some(&window_title));

    // Theme toggle button (right)
    let style = adw::StyleManager::default();
    let theme_icon_name = if style.is_dark() {
        "display-brightness-symbolic"
    } else {
        "weather-clear-night-symbolic"
    };
    let theme_btn = Button::from_icon_name(theme_icon_name);
    theme_btn.set_tooltip_text(Some("Cambiar tema claro / oscuro"));
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
    app_menu_btn.set_tooltip_text(Some("Menú principal"));
    app_menu_btn.set_valign(gtk::Align::Center);
    app_menu_btn.set_menu_model(Some(&build_app_menu(sender)));
    bar.pack_end(&app_menu_btn);

    HeaderBarWidgets {
        container: bar,
        back_btn,
        device_switcher: switcher,
        device_label,
        window_title,
    }
}

pub fn build_app_menu(_sender: &flume::Sender<AppMsg>) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    let section = gtk::gio::Menu::new();
    section.append(Some("Actualizar información"), Some("win.refresh"));
    section.append(Some("Mis dispositivos Bose"), Some("win.mybose"));
    section.append(Some("Acerca de Bose Connect"), Some("win.about"));
    section.append(Some("Salir"), Some("win.quit"));
    menu.append_section(None, &section);
    menu
}

pub fn show_about_dialog(parent: &impl glib::object::IsA<gtk::Window>) {
    let dialog = adw::AboutDialog::builder()
        .application_name("Bose Connect")
        .developer_name("airvzxf")
        .version(env!("CARGO_PKG_VERSION"))
        .comments("Control de auriculares y altavoces Bose sobre RFCOMM Bluetooth en Linux.\nMotor de protocolo: bose-connect v0.1.0")
        .website("https://github.com/airvzxf/bose-connect-app-linux")
        .issue_url("https://github.com/airvzxf/bose-connect-app-linux/issues")
        .license_type(gtk::License::Gpl30)
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

        let (battery, name, address_str) = match &model.snapshot {
            Some(s) => {
                let mac = s.device_bd_addr.unwrap_or(s.address);
                (s.battery as i32, s.name.clone(), format_address(mac))
            }
            None => (-1, "Desconectado".to_string(), "—".to_string()),
        };

        // Left icon/avatar
        let icon_box = GtkBox::new(Orientation::Horizontal, 0);
        icon_box.add_css_class("hero-avatar");
        icon_box.set_size_request(56, 56);
        icon_box.set_valign(gtk::Align::Center);
        icon_box.set_halign(gtk::Align::Center);
        let ic_name = if name.contains("SLC") {
            "audio-speakers-symbolic"
        } else {
            "audio-headphones-symbolic"
        };
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
            edit_btn.set_tooltip_text(Some("Renombrar dispositivo"));
            let s_edit = sender.clone();
            let n_owned = name.clone();
            edit_btn.connect_clicked(move |b| {
                open_rename_dialog(b, &n_owned, &s_edit);
            });
            title_row.append(&edit_btn);
        }
        mid.append(&title_row);

        let sub_text = match &model.snapshot {
            Some(s) if s.device_bd_addr.is_some() => {
                format!("Conectado vía RFCOMM · BD_ADDR: {address_str}")
            }
            _ => format!("Conectado vía RFCOMM · MAC: {address_str}"),
        };
        let sub_label = Label::new(Some(&sub_text));
        sub_label.set_xalign(0.0);
        sub_label.add_css_class("row-desc");
        mid.append(&sub_label);

        let pills_box = GtkBox::new(Orientation::Horizontal, 6);
        let status_pill = Label::new(Some(match model.connection {
            ConnectionState::Connected => "● Conectado",
            _ => "○ Desconectado",
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

        let tip = Label::new(Some("últimas 32 lecturas"));
        tip.add_css_class("row-desc");
        tip.set_xalign(1.0);
        right.append(&tip);

        card.append(&right);

        card.upcast::<gtk::Widget>()
    }
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

        // 1. Hero Card
        page.append(&HeroCard::render(model, sender));

        // 2. Audio y Reproducción (Media & Volume)
        if let Some(snapshot) = &model.snapshot {
            let audio_group = adw::PreferencesGroup::new();
            audio_group.set_title("Audio y Reproducción");

            // Fila 1: Control multimedia
            let media_row = adw::ActionRow::new();
            media_row.set_title("Reproducción");
            media_row.set_subtitle("Pausa/reanuda o navega entre pistas de audio");
            media_row.add_prefix(&Image::from_icon_name("media-playback-start-symbolic"));

            let btn_box = GtkBox::new(Orientation::Horizontal, 0);
            btn_box.add_css_class("linked");

            let prev_btn = Button::builder()
                .icon_name("media-skip-backward-symbolic")
                .tooltip_text("Pista anterior")
                .build();
            let s_prev = sender.clone();
            prev_btn.connect_clicked(move |_| {
                s_prev.send_app(AppMsg::SendMediaKey(bose_connect::MediaKey::Previous));
            });
            btn_box.append(&prev_btn);

            let pause_btn = Button::builder()
                .icon_name("media-playback-start-symbolic")
                .tooltip_text("Reproducir")
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
                    pause_btn_clone.set_tooltip_text(Some("Pausar"));
                } else {
                    pause_btn_clone.set_icon_name("media-playback-start-symbolic");
                    pause_btn_clone.set_tooltip_text(Some("Reproducir"));
                }
                s_pause.send_app(AppMsg::SendMediaKey(bose_connect::MediaKey::Pause));
            });
            btn_box.append(&pause_btn);

            let next_btn = Button::builder()
                .icon_name("media-skip-forward-symbolic")
                .tooltip_text("Pista siguiente")
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
                vol_row.set_title("Volumen del auricular");
                vol_row.set_subtitle("Nivel de volumen nativo de Bose (0 a 75)");

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
                let vol_lbl = Label::new(Some(&format!("{vol} / 75 ({pct}%)")));
                vol_lbl.set_size_request(110, -1);
                vol_lbl.set_xalign(1.0);
                vol_lbl.add_css_class("row-value");

                let s_vol = sender.clone();
                let vol_img_clone = vol_img.clone();
                let vol_lbl_clone = vol_lbl.clone();
                scale.connect_value_changed(move |s| {
                    let v = s.value().clamp(0.0, 75.0).round() as u8;
                    let p = (v as f64 / 75.0 * 100.0).round() as u32;
                    vol_lbl_clone.set_text(&format!("{v} / 75 ({p}%)"));
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
            headset_group.set_title("Control del Auricular");

            if cap.noise_cancelling {
                let anc_row = adw::ActionRow::new();
                anc_row.set_title("Cancelación activa de ruido (ANC)");
                anc_row.set_subtitle("Nivel de atenuación acústica exterior");
                anc_row.add_prefix(&Image::from_icon_name("audio-volume-muted-symbolic"));

                let current_nc = match model.snapshot.as_ref().map(|s| s.status.level) {
                    Some(bose_connect::NoiseCancelling::High) => "high",
                    Some(bose_connect::NoiseCancelling::Low) => "low",
                    _ => "off",
                };
                let s_nc = sender.clone();
                let seg = segmented_control(
                    &[("off", "Desactivado"), ("low", "Bajo"), ("high", "Alto")],
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
                sv_row.set_title("Voz propia (Self-Voice)");
                sv_row.set_subtitle("Escucha tu propia voz con naturalidad durante llamadas");
                sv_row.add_prefix(&Image::from_icon_name("audio-input-microphone-symbolic"));
                let sv_strings = StringList::new(&["Desactivado", "Bajo", "Medio", "Alto"]);
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
        voice_group.set_title("Voz e Indicaciones");

        let voice_on = matches!(
            model.snapshot.as_ref(),
            Some(s) if s.status.language & bose_connect::VP_ENABLE_BIT != 0
        );
        let vp_row = adw::SwitchRow::new();
        vp_row.set_title("Indicaciones de voz (Voice Prompts)");
        vp_row.set_subtitle("Anuncios hablados de estado y batería");
        vp_row.add_prefix(&Image::from_icon_name("audio-volume-high-symbolic"));
        vp_row.set_active(voice_on);
        let s_vp = sender.clone();
        vp_row.connect_active_notify(move |r| {
            s_vp.send_app(AppMsg::SetVoicePrompts(r.is_active()));
        });
        voice_group.add(&vp_row);

        let lang_row = adw::ComboRow::new();
        lang_row.set_title("Idioma de las indicaciones");
        lang_row.set_subtitle("Idioma del sintetizador vocal interno");
        lang_row.add_prefix(&Image::from_icon_name(
            "preferences-desktop-locale-symbolic",
        ));
        let lang_choices = [
            "English",
            "Français",
            "Italiano",
            "Deutsch",
            "Español",
            "Português",
            "中文",
            "한국어",
            "Русский",
            "Polski",
            "Nederlands",
            "日本語",
            "Svenska",
        ];
        let lang_model = StringList::new(&lang_choices);
        lang_row.set_model(Some(&lang_model));

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
        if let Some(pos) = lang_choices.iter().position(|l| *l == current_lang) {
            lang_row.set_selected(pos as u32);
        }

        let s_lang = sender.clone();
        lang_row.connect_selected_notify(move |r| {
            let idx = r.selected() as usize;
            let lang = match idx {
                0 => bose_connect::PromptLanguage::En,
                1 => bose_connect::PromptLanguage::Fr,
                2 => bose_connect::PromptLanguage::It,
                3 => bose_connect::PromptLanguage::De,
                4 => bose_connect::PromptLanguage::Es,
                5 => bose_connect::PromptLanguage::Pt,
                6 => bose_connect::PromptLanguage::Zh,
                7 => bose_connect::PromptLanguage::Ko,
                8 => bose_connect::PromptLanguage::Ru,
                9 => bose_connect::PromptLanguage::Pl,
                10 => bose_connect::PromptLanguage::Nl,
                11 => bose_connect::PromptLanguage::Ja,
                12 => bose_connect::PromptLanguage::Sv,
                _ => bose_connect::PromptLanguage::En,
            };
            s_lang.send_app(AppMsg::SetLanguage {
                language: lang,
                voice_prompts: true,
            });
        });
        voice_group.add(&lang_row);
        page.append(&voice_group);

        // 5. Energía y Batería
        let power_group = adw::PreferencesGroup::new();
        power_group.set_title("Energía y Batería");

        let ao_row = adw::ComboRow::new();
        ao_row.set_title("Apagado automático");
        ao_row.set_subtitle("Suspende el dispositivo tras inactividad para ahorrar batería");
        ao_row.add_prefix(&Image::from_icon_name("clock-symbolic"));
        let ao_choices = ["Nunca", "5 min", "20 min", "40 min", "60 min", "180 min"];
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
        mp_group.set_title("Dispositivos Vinculados al Bose (Multipunto)");

        let pair_row = adw::SwitchRow::new();
        pair_row.set_title("Visible para emparejar");
        pair_row.set_subtitle("Permite a otros teléfonos o computadoras descubrir este Bose");
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
            row.set_title("No hay otros dispositivos vinculados");
            row.set_subtitle("Activa 'Visible para emparejar' para conectar nuevos equipos");
            row.add_prefix(&Image::from_icon_name("dialog-information-symbolic"));
            mp_group.add(&row);
        } else {
            let active_addr = model.snapshot.as_ref().and_then(|s| s.active_device);
            for dev in devices {
                let name = match std::str::from_utf8(&dev.name[..dev.name_len]) {
                    Ok(s) => s.to_string(),
                    Err(_) => format!("Device {:02X?}", dev.address.b),
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
                        let pill = Label::new(Some("Este equipo"));
                        pill.add_css_class("pill");
                        pill.add_css_class("success");
                        row.add_suffix(&pill);

                        if active_addr == Some(dev.address) {
                            let act_pill = Label::new(Some("Audio activo"));
                            act_pill.add_css_class("pill");
                            act_pill.add_css_class("accent");
                            row.add_suffix(&act_pill);
                        }

                        let dc_btn = Button::with_label("Desconectar");
                        dc_btn.add_css_class("flat");
                        let s = sender.clone();
                        dc_btn.connect_clicked(move |_| {
                            s.send_app(AppMsg::PairedDisconnect(dev.address));
                        });
                        row.add_suffix(&dc_btn);
                    }
                    bose_connect::DeviceStatus::Connected => {
                        let pill = Label::new(Some("Conectado"));
                        pill.add_css_class("pill");
                        pill.add_css_class("success");
                        row.add_suffix(&pill);

                        if active_addr == Some(dev.address) {
                            let act_pill = Label::new(Some("Audio activo"));
                            act_pill.add_css_class("pill");
                            act_pill.add_css_class("accent");
                            row.add_suffix(&act_pill);
                        }

                        let dc_btn = Button::with_label("Desconectar");
                        dc_btn.add_css_class("flat");
                        let s = sender.clone();
                        dc_btn.connect_clicked(move |_| {
                            s.send_app(AppMsg::PairedDisconnect(dev.address));
                        });
                        row.add_suffix(&dc_btn);

                        let forget_btn = Button::from_icon_name("user-trash-symbolic");
                        forget_btn.add_css_class("flat");
                        forget_btn.set_tooltip_text(Some("Olvidar dispositivo"));
                        let s_del = sender.clone();
                        let n_owned = name.clone();
                        forget_btn.connect_clicked(move |b| {
                            open_forget_dialog(b, &n_owned, dev.address, &s_del);
                        });
                        row.add_suffix(&forget_btn);
                    }
                    bose_connect::DeviceStatus::Disconnected => {
                        let cn_btn = Button::with_label("Conectar");
                        cn_btn.add_css_class("suggested-action");
                        let s = sender.clone();
                        cn_btn.connect_clicked(move |_| {
                            s.send_app(AppMsg::PairedConnect(dev.address));
                        });
                        row.add_suffix(&cn_btn);

                        let forget_btn = Button::from_icon_name("user-trash-symbolic");
                        forget_btn.add_css_class("flat");
                        forget_btn.set_tooltip_text(Some("Olvidar dispositivo"));
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
        info_group.set_title("Información del Dispositivo");

        let (device_id, serial, firmware, bd_addr) = match &model.snapshot {
            Some(s) => (
                format!("0x{:04X}  ·  Índice de revisión: 2", s.device_id),
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
        id_row.set_title("Identificador de hardware");
        id_row.add_prefix(&Image::from_icon_name("dialog-information-symbolic"));
        let id_lbl = Label::new(Some(&device_id));
        id_lbl.set_selectable(true);
        id_lbl.add_css_class("row-value");
        id_row.add_suffix(&id_lbl);
        info_group.add(&id_row);

        let mac_row = adw::ActionRow::new();
        mac_row.set_title("Dirección Bluetooth física (BD_ADDR)");
        mac_row.set_subtitle("Dirección MAC de hardware del módulo Bose");
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
        fw_row.set_title("Versión de firmware");
        fw_row.add_prefix(&Image::from_icon_name("system-software-update-symbolic"));
        let fw_lbl = Label::new(Some(&firmware));
        fw_lbl.set_selectable(true);
        fw_lbl.add_css_class("row-value");
        fw_row.add_suffix(&fw_lbl);
        info_group.add(&fw_row);

        let sn_row = adw::ActionRow::new();
        sn_row.set_title("Número de serie");
        sn_row.add_prefix(&Image::from_icon_name("emblem-system-symbolic"));

        let sn_box = GtkBox::new(Orientation::Horizontal, 8);
        let sn_lbl = Label::new(Some("•••••••••••••••••"));
        sn_lbl.set_selectable(true);
        sn_lbl.add_css_class("mono");
        sn_box.append(&sn_lbl);

        let reveal_btn = Button::from_icon_name("view-reveal-symbolic");
        reveal_btn.add_css_class("flat");
        reveal_btn.set_tooltip_text(Some("Mostrar número de serie"));
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
                    btn_for_click.set_tooltip_text(Some("Ocultar número de serie"));
                } else {
                    sn_lbl.set_text("•••••••••••••••••");
                    btn_for_click.set_icon_name("view-reveal-symbolic");
                    btn_for_click.set_tooltip_text(Some("Mostrar número de serie"));
                }
            });
        }
        sn_box.append(&reveal_btn);
        sn_row.add_suffix(&sn_box);
        info_group.add(&sn_row);

        page.append(&info_group);

        // 8. Avanzado
        let adv_group = adw::PreferencesGroup::new();
        adv_group.set_title("Avanzado");

        let packet_row = adw::ActionRow::new();
        packet_row.set_title("Inyección de paquetes");
        packet_row.set_subtitle("Envía tramas raw hexadecimales sobre el canal RFCOMM");
        packet_row.add_prefix(&Image::from_icon_name("utilities-terminal-symbolic"));
        let input_box = GtkBox::new(Orientation::Horizontal, 8);
        let packet_entry = gtk::Entry::new();
        packet_entry.set_placeholder_text(Some("0a1b2c3d"));
        packet_entry.add_css_class("mono");
        input_box.append(&packet_entry);
        let send_btn = Button::with_label("Enviar");
        send_btn.add_css_class("suggested-action");
        let s_packet = sender.clone();
        send_btn.connect_clicked(move |_| {
            s_packet.send_app(AppMsg::Acknowledge);
        });
        input_box.append(&send_btn);
        packet_row.add_suffix(&input_box);
        adv_group.add(&packet_row);

        let resp_row = adw::ActionRow::new();
        resp_row.set_title("Última respuesta");
        resp_row.set_subtitle("Hexadecimal recibido del dispositivo");
        resp_row.add_prefix(&Image::from_icon_name("network-receive-symbolic"));
        let resp_box = GtkBox::new(Orientation::Vertical, 4);
        resp_box.set_valign(gtk::Align::Center);
        let resp_lbl = Label::new(Some("01 00 04 00 aa bb cc dd 10 20 30 40 50 60 70 80"));
        resp_lbl.set_selectable(true);
        resp_lbl.set_wrap(true);
        resp_lbl.set_wrap_mode(gtk::pango::WrapMode::Char);
        resp_lbl.set_max_width_chars(36);
        resp_lbl.set_xalign(0.0);
        resp_lbl.add_css_class("mono");
        resp_lbl.add_css_class("packet-response-box");
        resp_box.append(&resp_lbl);
        resp_row.add_suffix(&resp_box);
        adv_group.add(&resp_row);

        let ver_row = adw::ActionRow::new();
        ver_row.set_title("Versiones del sistema");
        ver_row.add_prefix(&Image::from_icon_name("system-software-update-symbolic"));
        let ver_lbl = Label::new(Some(&format!(
            "bose-connect-gui v{}  ·  bose-connect v0.1.0",
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
// MyBosePage (Laptop-side Bose device manager & scanner)
// ---------------------------------------------------------------------------

pub struct MyBosePage;

impl MyBosePage {
    pub fn render(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
        let page = GtkBox::new(Orientation::Vertical, 20);
        page.set_hexpand(true);
        page.set_vexpand(true);

        // Header section
        let header_group = adw::PreferencesGroup::new();
        header_group.set_title("Mis Dispositivos Bose");

        // Active device
        let (active_name, active_addr) = match &model.snapshot {
            Some(s) => {
                let mac = s.device_bd_addr.unwrap_or(s.address);
                (s.name.clone(), format_address(mac))
            }
            None => ("Ningún dispositivo conectado".to_string(), "—".to_string()),
        };

        let active_row = adw::ActionRow::new();
        active_row.set_title(&active_name);
        active_row.set_subtitle(&active_addr);
        let active_icon = if active_name.contains("SLC") {
            "audio-speakers-symbolic"
        } else {
            "audio-headphones-symbolic"
        };
        active_row.add_prefix(&Image::from_icon_name(active_icon));

        let active_pill = Label::new(Some("Activo"));
        active_pill.add_css_class("pill");
        active_pill.add_css_class("success");
        active_row.add_suffix(&active_pill);

        let dc_btn = Button::with_label("Desconectar");
        dc_btn.add_css_class("flat");
        let s_dc = sender.clone();
        dc_btn.connect_clicked(move |_| {
            s_dc.send_app(AppMsg::Acknowledge);
        });
        active_row.add_suffix(&dc_btn);
        header_group.add(&active_row);

        // Other known devices
        let other_devices = [
            ("Bose SLC II Black 🐺", "2C:41:A1:0B:7C:82"),
            ("Bose QuietComfort Earbuds", "CC:DD:EE:FF:00:11"),
        ];

        for (dname, daddr) in other_devices {
            let row = adw::ActionRow::new();
            row.set_title(dname);
            row.set_subtitle(daddr);
            let row_icon = if dname.contains("SLC") {
                "audio-speakers-symbolic"
            } else {
                "audio-headphones-symbolic"
            };
            row.add_prefix(&Image::from_icon_name(row_icon));

            let pill = Label::new(Some("Enlazado"));
            pill.add_css_class("pill");
            pill.add_css_class("muted");
            row.add_suffix(&pill);

            let cn_btn = Button::with_label("Usar este");
            cn_btn.add_css_class("suggested-action");
            let s_cn = sender.clone();
            let target_addr = daddr.to_string();
            cn_btn.connect_clicked(move |_| {
                s_cn.send_app(AppMsg::Connect(target_addr.clone()));
            });
            row.add_suffix(&cn_btn);

            let forget_btn = Button::from_icon_name("user-trash-symbolic");
            forget_btn.add_css_class("flat");
            forget_btn.set_tooltip_text(Some("Olvidar de esta laptop"));
            row.add_suffix(&forget_btn);

            header_group.add(&row);
        }

        page.append(&header_group);

        // Discovery section
        let scan_group = adw::PreferencesGroup::new();
        scan_group.set_title("Vincular Nuevo Dispositivo Bose");

        let scan_row = adw::ActionRow::new();
        scan_row.set_title("Buscar dispositivos Bose cercanos");
        scan_row.add_prefix(&Image::from_icon_name("network-wireless-symbolic"));

        let scan_btn = Button::with_label("Escanear");
        scan_btn.add_css_class("suggested-action");
        let s_scan = sender.clone();
        scan_btn.connect_clicked(move |_| {
            s_scan.send_app(AppMsg::Acknowledge);
        });
        scan_row.add_suffix(&scan_btn);
        scan_group.add(&scan_row);

        // If discovery has items
        if !model.discovery.is_empty() {
            for (addr, name) in &model.discovery {
                let row = adw::ActionRow::new();
                row.set_title(name);
                row.set_subtitle(addr);
                row.add_prefix(&Image::from_icon_name("audio-headphones-symbolic"));
                let pair_btn = Button::with_label("Emparejar");
                pair_btn.add_css_class("suggested-action");
                let s_pair = sender.clone();
                let addr_clone = addr.clone();
                pair_btn.connect_clicked(move |_| {
                    s_pair.send_app(AppMsg::Connect(addr_clone.clone()));
                });
                row.add_suffix(&pair_btn);
                scan_group.add(&row);
            }
        }

        page.append(&scan_group);

        page.upcast::<gtk::Widget>()
    }
}
