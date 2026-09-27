//! Helpers that construct the widget tree.

use adw;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Orientation};

use crate::app::model::{AppModel, AppMsg};
use crate::app::widgets::{
    ActivityLog, ConnectionBanner, HeroCard, NoiseCancellingTile, PairedDevicesPanel, ProfilesBar,
    QuietModePill, SenderExt,
};

/// Build the top-level widget for the main window.
pub fn build_root(
    app: &adw::Application,
    model: &AppModel,
    sender: &flume::Sender<AppMsg>,
) -> gtk::Widget {
    let toolbar_view = adw::ToolbarView::new();

    let top_bar = build_top_bar(app, model, sender);
    toolbar_view.add_top_bar(&top_bar);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_margin_top(16);
    content.set_margin_bottom(24);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_vexpand(true);
    content.set_hexpand(true);

    let clamped = adw::Clamp::new();
    clamped.set_maximum_size(960);
    clamped.set_tightening_threshold(820);
    clamped.set_child(Some(&content));
    toolbar_view.set_content(Some(&clamped));

    content.append(&ConnectionBanner::render(model));
    content.append(&HeroCard::render(model));
    content.append(&build_quick_settings_row(model, sender));

    let split = GtkBox::new(Orientation::Horizontal, 16);
    split.set_margin_top(16);
    split.set_hexpand(true);
    split.set_homogeneous(true);
    split.append(&ProfilesBar::render(model, sender));
    split.append(&PairedDevicesPanel::render(model));
    content.append(&split);

    content.append(&ActivityLog::render(model));

    toolbar_view.upcast::<gtk::Widget>()
}

pub fn build_top_bar(
    app: &adw::Application,
    model: &AppModel,
    sender: &flume::Sender<AppMsg>,
) -> adw::HeaderBar {
    let bar = adw::HeaderBar::new();
    let title_widget = adw::WindowTitle::new("Bose Connect", "");
    bar.set_title_widget(Some(&title_widget));

    let menu = gtk::MenuButton::new();
    menu.set_icon_name("open-menu-symbolic");
    menu.set_menu_model(Some(&crate::app::widgets::build_app_menu(
        app.upcast_ref::<gtk::Application>(),
    )));
    bar.pack_end(&menu);

    let refresh_button = gtk::Button::with_label("Refresh");
    refresh_button.add_css_class("quick-action");
    refresh_button.set_tooltip_text(Some("Re-read every setting from the device"));
    let s = sender.clone();
    refresh_button.connect_clicked(move |_| {
        s.send_app(AppMsg::Refresh);
    });
    bar.pack_end(&refresh_button);

    bar.pack_start(&QuietModePill::render(model, sender));

    bar
}

fn build_quick_settings_row(model: &AppModel, sender: &flume::Sender<AppMsg>) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 16);
    row.set_margin_top(12);
    row.set_hexpand(true);
    row.set_homogeneous(true);

    row.append(&NoiseCancellingTile::render_tile_nc(model, sender));
    row.append(&NoiseCancellingTile::render_tile_voice(model, sender));
    row.append(&NoiseCancellingTile::render_tile_language(model, sender));
    row.append(&NoiseCancellingTile::render_tile_auto_off(model, sender));

    row
}
