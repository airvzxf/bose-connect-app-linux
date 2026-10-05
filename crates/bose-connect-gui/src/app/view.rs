//! Top-level widget assembly.
//!
//! The window layout uses `AdwToolbarView`:
//!   - Top bar: HeaderBar with device switcher, title, theme toggle, and menu.
//!   - Content: A clamped, scrollable view (`AdwClamp` inside `GtkScrolledWindow`)
//!     swapping between `OverviewPage` (the unified control center) and `MyBosePage` (laptop Bose manager).

use adw::prelude::*;
use gtk::{glib, Box as GtkBox, Orientation};

use crate::app::model::{AppModel, AppMsg, Page};
use crate::app::widgets::{build_header_bar, HeaderBarWidgets, MyBosePage, OverviewPage};

/// Closure type used by [`build_root`] so the caller can rebuild
/// the page content from anywhere (typically from the message
/// pump). We hand out a small struct that wraps the closure so
/// the lifetime story stays simple.
pub struct RenderHandle {
    rebuild: Box<dyn Fn()>,
}

impl RenderHandle {
    pub fn rebuild(&self) {
        (self.rebuild)();
    }
}

/// Build the top-level widget, the page-content `GtkBox` (used
/// only as a reference for size hints), and a `RenderHandle`
/// for swapping the active page in place.
pub fn build_root(
    app: &adw::Application,
    model: &AppModel,
    sender: &flume::Sender<AppMsg>,
    model_holder: &std::rc::Rc<std::cell::RefCell<AppModel>>,
) -> (gtk::Widget, GtkBox, RenderHandle) {
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.set_top_bar_style(adw::ToolbarStyle::Flat);

    let header_widgets = build_header_bar(model, sender);
    toolbar_view.add_top_bar(&header_widgets.container);

    let content_scroll = gtk::ScrolledWindow::new();
    content_scroll.set_hexpand(true);
    content_scroll.set_vexpand(true);
    content_scroll.set_vscrollbar_policy(gtk::PolicyType::Automatic);
    content_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(740);
    clamp.set_tightening_threshold(600);
    clamp.set_hexpand(true);
    clamp.set_vexpand(true);

    let content_box = GtkBox::new(Orientation::Vertical, 0);
    content_box.set_margin_top(16);
    content_box.set_margin_bottom(28);
    content_box.set_margin_start(16);
    content_box.set_margin_end(16);
    content_box.set_hexpand(true);
    content_box.set_vexpand(true);

    content_box.append(&render_active_page(model, sender));
    clamp.set_child(Some(&content_box));
    content_scroll.set_child(Some(&clamp));

    toolbar_view.set_content(Some(&content_scroll));

    let _ = app;

    let render_handle = build_render_handle(model_holder, sender, &content_box, header_widgets);

    (
        toolbar_view.upcast::<gtk::Widget>(),
        content_box,
        render_handle,
    )
}

/// Build the closure that swaps the active page in place.
fn build_render_handle(
    model_holder: &std::rc::Rc<std::cell::RefCell<AppModel>>,
    sender: &flume::Sender<AppMsg>,
    content_box: &GtkBox,
    header_widgets: HeaderBarWidgets,
) -> RenderHandle {
    let model_weak = std::rc::Rc::downgrade(model_holder);
    let content_box_weak = content_box.downgrade();
    let sender = sender.clone();

    RenderHandle {
        rebuild: Box::new(move || {
            let content_box = match content_box_weak.upgrade() {
                Some(w) => w,
                None => return,
            };
            let model = match model_weak.upgrade() {
                Some(m) => m,
                None => return,
            };

            let mut child = content_box.first_child();
            let mut cleared = 0;
            while let Some(c) = child {
                content_box.remove(&c);
                cleared += 1;
                child = content_box.first_child();
            }
            let snapshot = model.borrow();
            let active_page = snapshot.current_page;
            let new_widget = render_active_page(&snapshot, &sender);
            content_box.append(&new_widget);

            // Update header bar controls
            header_widgets
                .back_btn
                .set_visible(active_page == Page::MyBose);
            header_widgets
                .device_switcher
                .set_visible(active_page == Page::Overview);
            header_widgets
                .window_title
                .set_subtitle(active_page.subtitle());

            if let Some(s) = &snapshot.snapshot {
                header_widgets.device_label.set_label(&s.name);
            } else {
                header_widgets.device_label.set_label("Desconectado");
            }

            tracing::info!(
                target: "rebuild",
                "rebuild fired: cleared {cleared} children, appended page {}",
                active_page.title()
            );

            let _ = glib::ControlFlow::Continue;
        }),
    }
}

/// Render the page that the model says is active.
pub fn render_active_page(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
    match model.current_page {
        Page::Overview => OverviewPage::render(model, sender),
        Page::MyBose => MyBosePage::render(model, sender),
    }
}
