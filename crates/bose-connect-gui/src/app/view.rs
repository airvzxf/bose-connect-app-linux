//! Top-level widget assembly.
//!
//! The window is an `Adw::NavigationSplitView` with:
//!
//!   - a left sidebar showing the page list (wrapped in an
//!     `Adw::NavigationPage` because that's what
//!     `NavigationSplitView::set_sidebar` accepts),
//!   - a right content area that swaps between pages,
//!
//! wrapped in an `Adw::ToolbarView` so the HeaderBar lives on top.
//!
//! `build_root` returns the root widget *and* a [`RenderHandle`]
//! the caller can invoke to swap the active page in place.

use adw::prelude::*;
use gtk::{glib, Box as GtkBox, Orientation};

use crate::app::model::{AppModel, AppMsg, Page};
use crate::app::widgets::{
    build_header_bar, AdvancedPage, AudioPage, ConnectionBanner, DevicePage, MultipointPage,
    MyBosePage, OverviewPage, Sidebar,
};

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

    let top_bar = build_header_bar(model, sender);
    toolbar_view.add_top_bar(&top_bar);

    // Main content: status banner + sidebar + page area.
    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_hexpand(true);
    outer.set_vexpand(true);

    // Slim connection banner above the split view.
    let banner = ConnectionBanner::render(model);
    outer.append(&banner);

    let split = adw::NavigationSplitView::new();
    split.set_hexpand(true);
    split.set_vexpand(true);
    split.set_sidebar_width_fraction(0.22);
    split.set_min_sidebar_width(200.0);
    split.set_max_sidebar_width(260.0);
    // The sidebar is always visible (not collapsible into a hamburger)
    // because the window is wide enough on every supported desktop
    // and toggling it disrupts the visual rhythm.
    split.set_collapsed(false);

    // Sidebar: wrap the ListBox in a NavigationPage so the
    // split view can hold it. We also keep a strong reference to
    // the inner ListBox so `RenderHandle::rebuild` can move the
    // `.selected` class to the row matching the new page —
    // `Sidebar::render` paints `.selected` based on the *initial*
    // model, but `current_page` changes after `NavigateTo` and
    // the sidebar would otherwise stay on the first row forever.
    let (sidebar_widget, sidebar_list) = Sidebar::render_with_list(model, sender);
    let sidebar_page = adw::NavigationPage::new(&sidebar_widget, "Navigation");
    sidebar_page.add_css_class("sidebar-page");
    split.set_sidebar(Some(&sidebar_page));

    // Content frame: a ScrolledWindow holding a Clamp of the current page.
    let content_scroll = gtk::ScrolledWindow::new();
    content_scroll.set_hexpand(true);
    content_scroll.set_vexpand(true);
    content_scroll.set_vscrollbar_policy(gtk::PolicyType::Automatic);
    content_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);

    let content_box = GtkBox::new(Orientation::Vertical, 0);
    content_box.set_margin_top(16);
    content_box.set_margin_bottom(24);
    content_box.set_margin_start(20);
    content_box.set_margin_end(20);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(820);
    clamp.set_tightening_threshold(720);
    clamp.set_child(Some(&content_box));
    content_scroll.set_child(Some(&clamp));

    // Render the active page; we always re-render the whole tree
    // on every `apply`, since the model is a flat struct.
    content_box.append(&render_active_page(model, sender));

    let content_page = adw::NavigationPage::new(&content_scroll, model.current_page.title());
    content_page.add_css_class("content-page");
    split.set_content(Some(&content_page));

    outer.append(&split);
    toolbar_view.set_content(Some(&outer));

    let _ = app;

    let render_handle = build_render_handle(model_holder, sender, &content_box, &sidebar_list);

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
    sidebar_list: &gtk::ListBox,
) -> RenderHandle {
    // Capture by `downgrade()` so the closure does not keep the
    // window alive after the user has closed it.
    let model_weak = std::rc::Rc::downgrade(model_holder);
    let content_box_weak = content_box.downgrade();
    let sidebar_list_weak = sidebar_list.downgrade();
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
            // Walk `first_child` because `GtkBox` doesn't expose a
            // `clear()` in gtk4-rs.
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
            tracing::info!(
                target: "rebuild",
                "rebuild fired: cleared {cleared} children, appended page {}",
                active_page.title()
            );

            // Move the `.selected` CSS class on the sidebar rows so
            // the user sees which page they're on. Sidebar::render
            // paints the class once at startup; without this dance
            // the highlight stays on the first page forever.
            if let Some(sidebar) = sidebar_list_weak.upgrade() {
                let mut row = sidebar.first_child();
                while let Some(r) = row.clone() {
                    if let Ok(list_row) = r.clone().downcast::<gtk::ListBoxRow>() {
                        let name = list_row.widget_name();
                        if name == active_page.key() {
                            list_row.add_css_class("selected");
                        } else {
                            list_row.remove_css_class("selected");
                        }
                    }
                    row = r.next_sibling();
                }
            }
            // Drop the warning from glib if the closure is
            // invoked from a non-glib context — some pumps
            // fire from the tokio worker thread.
            let _ = glib::ControlFlow::Continue;
        }),
    }
}

/// Render the page that the model says is active. Pure function
/// over `model` — the widget tree is rebuilt every time the model
/// changes (the existing pump is a one-shot apply, no diffing).
pub fn render_active_page(model: &AppModel, sender: &flume::Sender<AppMsg>) -> gtk::Widget {
    match model.current_page {
        Page::MyBose => MyBosePage::render(model, sender),
        Page::Overview => OverviewPage::render(model, sender),
        Page::Audio => AudioPage::render(model, sender),
        Page::Device => DevicePage::render(model, sender),
        Page::Multipoint => MultipointPage::render(model, sender),
        Page::Advanced => AdvancedPage::render(model, sender),
    }
}
