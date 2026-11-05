//! `bose-connect-gui` — launch the GTK4 application.

use std::sync::Arc;

#[allow(unused_imports)]
use adw::prelude::*;
use gtk::glib;
#[allow(unused_imports)]
use gtk::prelude::*;
use once_cell::sync::OnceCell;

use bose_connect_gui::app::model::{AppModel, AppMsg, ConnectionState, Page, TICK_INTERVAL_MS};
use bose_connect_gui::app::view::{build_root, RenderHandle};
use bose_connect_gui::services::bluetooth::PairedBose;
use bose_connect_gui::services::device::{
    DeviceService, DeviceSnapshot, MockScenario, MockService, RealService,
};
use bose_connect_gui::services::notifications::{NotificationLevel, Notifications};
use bose_connect_gui::transport;

/// How often the GUI re-reads a real device.
const REAL_REFRESH_MS: u64 = 60_000;

/// Set to `1` (or `connected`, `disconnected`, `no-devices`) to run
/// against the simulated device instead of a real one, in that
/// scenario. For tests and screenshots only, so it is not in `--help`.
const SIMULATOR_ENV: &str = "BOSE_CONNECT_GUI_SIMULATOR";

static RUNTIME: OnceCell<Arc<tokio::runtime::Runtime>> = OnceCell::new();

fn runtime() -> &'static Arc<tokio::runtime::Runtime> {
    RUNTIME.get_or_init(|| {
        Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .thread_name("bose-connect-gui")
                .build()
                .expect("tokio runtime"),
        )
    })
}

#[derive(Debug, Default, Clone)]
struct CliArgs {
    /// Use the simulated device (developers only): set by
    /// [`SIMULATOR_ENV`] or implied by `--low-battery-test`.
    simulator: Option<MockScenario>,
    headless: bool,
    screenshot: Option<String>,
    address: Option<String>,
    /// Override the simulator's battery-drain interval (hidden
    /// developer flag, used by the `--low-battery-test` smoke).
    mock_tick_ms: Option<u64>,
    /// If set, the app keeps running for this many seconds and then
    /// exits cleanly. Used to drive notification flows without a
    /// window manager.
    run_secs: Option<u64>,
    /// Hidden developer flag: spawn a fresh simulator seeded with battery=10
    /// and a fast tick so the app fires a low-battery libnotify
    /// notification within a couple of seconds, then exits. The
    /// GUI is fully painted but the notification is the deliverable
    /// being tested.
    low_battery_test: bool,
    /// Cycle through every sidebar page on a fixed interval and
    /// dump the widget tree on each step. Used to verify that
    /// `AppMsg::NavigateTo` actually swaps the content area, even
    /// without a display server.
    tour: bool,
    /// Print the GTK widget tree to stdout once the window is built.
    dump_tree: bool,
    /// Start on a specific page: overview or mybose
    page: Option<String>,
    light: bool,
    dark: bool,
}

fn parse_cli() -> CliArgs {
    let mut cli = CliArgs {
        simulator: std::env::var(SIMULATOR_ENV)
            .ok()
            .and_then(|v| MockScenario::from_env_value(&v)),
        ..CliArgs::default()
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => cli.headless = true,
            "--screenshot" => cli.screenshot = args.next(),
            "--address" => cli.address = args.next(),
            "--mock-tick-ms" => cli.mock_tick_ms = args.next().and_then(|s| s.parse().ok()),
            "--run-secs" => cli.run_secs = args.next().and_then(|s| s.parse().ok()),
            "--low-battery-test" => {
                cli.low_battery_test = true;
                cli.simulator = cli.simulator.or(Some(MockScenario::Connected));
            }
            "--tour" => cli.tour = true,
            "--dump-tree" => cli.dump_tree = true,
            "--page" => cli.page = args.next(),
            "--light" => cli.light = true,
            "--dark" => cli.dark = true,
            "-h" | "--help" => {
                println!("bose-connect-gui — GTK4 GUI for Bose headphones and speakers");
                println!();
                println!("Usage: bose-connect-gui [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --headless                Render once and exit (smoke tests)");
                println!("  --screenshot PATH         Save a PPM of the first frame (headless)");
                println!("  --address AA:BB:CC:DD:EE:FF  Override the persisted address");
                println!("  --run-secs SECONDS        After running for N s, exit cleanly");
                println!("  --version, -V             Print version and exit");
                println!("  --dump-tree               Print the GTK widget tree to stdout");
                println!(
                    "  --tour                    Cycle through every page and dump its widget tree"
                );
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => eprintln!("warning: unknown argument: {other}"),
        }
    }

    cli
}

fn main() -> anyhow::Result<()> {
    let _rt = runtime();

    // Install the desktop file + icon SVGs into ~/.local/share/.
    // The first iteration of this branch shipped a libnotify call
    // with all four standard hints (DesktopEntry, ImagePath,
    // Category, Urgency), but until the .desktop file is on
    // $XDG_DATA_HOME/applications and the SVG is on
    // $XDG_DATA_HOME/icons/hicolor KDE Plasma 6 silently treats
    // the notification as an unknown-app transient overlay. The
    // install is idempotent: a content-hash check decides whether
    // a rewrite is needed, so subsequent launches are O(1).
    match bose_connect_gui::services::assets::install_all() {
        Ok(true) => tracing::info!(
            target: "assets",
            "installed/refreshed desktop + icon assets under $XDG_DATA_HOME",
        ),
        Ok(false) => tracing::debug!(
            target: "assets",
            "desktop + icon assets already in place; no rewrite",
        ),
        Err(err) => tracing::warn!(
            target: "assets",
            "could not install desktop + icon assets: {err}; KDE integration may be incomplete",
        ),
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    // Register the compiled GResource bundle (icons, CSS, GMenu)
    // with gio so `gtk::gio::resources_lookup_data(...)` resolves
    // the URI of the SVG the SNI IconPixmap property reads.
    // The registration is a one-shot, idempotent refcount bump.
    bose_connect_gui::register_resources();

    let cli = parse_cli();

    // The app always talks to real devices. The simulated device
    // is for tests and screenshots only.
    let mut _simulator_ticker = None;
    let _service: Arc<dyn DeviceService> = if let Some(scenario) = cli.simulator {
        // The simulator needs a Tokio runtime for its background
        // battery-drain tick task. The `--low-battery-test` flag
        // seeds the device with a 10 % charge so the libnotify
        // threshold-crossing fires within a few seconds of
        // `activate`.
        let seed = if cli.low_battery_test {
            Some(bose_connect_gui::transport::MockSeed {
                battery: 10,
                ..Default::default()
            })
        } else if scenario == MockScenario::Speaker {
            Some(bose_connect_gui::transport::MockSeed::speaker())
        } else {
            None
        };
        let tick = cli.mock_tick_ms.map(Duration::from_millis);
        let (handle, ticker) = runtime().block_on(async { transport::spawn(seed, tick) });
        // Dropping the ticker stops the battery drain.
        _simulator_ticker = Some(ticker);
        Arc::new(MockService::with_scenario(handle, scenario))
    } else {
        Arc::new(RealService::new())
    };

    let _ = gtk::init();

    glib::log_set_writer_func(|level, fields| {
        for field in fields {
            if let Some(val) = field.value_str() {
                if val.contains(
                    "Using GtkSettings:gtk-application-prefer-dark-theme with libadwaita is unsupported",
                ) {
                    return glib::LogWriterOutput::Handled;
                }
            }
        }
        glib::log_writer_default(level, fields)
    });

    let app = adw::Application::builder()
        .application_id("com.airvzxf.bose-connect-gui")
        .flags(gtk::gio::ApplicationFlags::empty())
        .build();

    let app_for_app = app.clone();
    let cli_clone = cli.clone();
    let service_for_activate = _service.clone();
    app.connect_activate(move |application| {
        // libadwaita replaces the GTK dark-mode setting with
        // its own `AdwStyleManager::color-scheme`. Setting the
        // legacy property on `GtkSettings` triggers an
        // `Adwaita-WARNING` at startup; we set the modern one
        // here so the application follows the system preference
        // without the warning.
        let style = adw::StyleManager::default();
        if cli_clone.light {
            style.set_color_scheme(adw::ColorScheme::ForceLight);
        } else if cli_clone.dark {
            style.set_color_scheme(adw::ColorScheme::ForceDark);
        } else {
            style.set_color_scheme(adw::ColorScheme::Default);
        }

        activate(
            &app_for_app,
            application.clone(),
            cli_clone.clone(),
            service_for_activate.clone(),
        );
    });

    let _ = app.run_with_args::<&str>(&[]);
    Ok(())
}

fn activate(
    app: &adw::Application,
    application: adw::Application,
    cli: CliArgs,
    _service: Arc<dyn DeviceService>,
) {
    // Install CSS from the GResource bundle so the styling
    // matches the production app.
    let provider = gtk::CssProvider::new();
    if let Ok(bytes) = gtk::gio::resources_lookup_data(
        "/com/airvzxf/bose-connect-gui/style.css",
        gtk::gio::ResourceLookupFlags::NONE,
    ) {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            provider.load_from_string(text);
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }

    let mut model = AppModel::new(_service.clone());
    model.simulator = cli.simulator.is_some();
    match _service.known_devices() {
        Ok(devices) => model.host_devices = Some(devices),
        Err(e) => {
            tracing::warn!(target: "bluez", "paired devices: {e}");
            model.log_error(format!("paired devices: {e}"));
        }
    }
    // Resume on the last page the user was on, if any. Defaults
    // to Page::MyBose for a fresh install.
    if let Some(last_page) = model
        .persisted
        .last_page
        .as_deref()
        .and_then(Page::from_key)
    {
        model.current_page = last_page;
    }
    if let Some(page) = cli.page.as_deref().and_then(Page::from_key) {
        model.current_page = page;
    }
    // The model starts empty and fills in from the device once
    // `AppMsg::Connect` (sent below) answers.
    let real_device = cli.simulator.is_none();
    if !real_device {
        model.log_info("Simulated device (developer mode)");
    }
    let connect_to = cli
        .address
        .clone()
        .or_else(|| startup_device(model.persisted.last_address.as_deref(), &model.host_devices));
    match &connect_to {
        Some(address) => model.connection = ConnectionState::Connecting(address.clone()),
        None => model.log_info("No device address yet: pick one or pass --address"),
    }

    // Set up a self-peeking channel so widgets can dispatch
    // messages back to the AppModel. We use a plain `flume`
    // sender here because the GUI is a one-writer / one-reader
    // pump.
    let (tx, rx) = flume::unbounded::<AppMsg>();
    if let Some(address) = connect_to {
        let _ = tx.send(AppMsg::Connect(address));
    }

    // Tray icon — hand-rolled StatusNotifierItem. We always start
    // it (even when running headless) so the smoke test can
    // verify the `RegisterStatusNotifierItem` call reaches
    // the watcher. When the session bus isn't available
    // (`DBusConnection::get` returns an error) we degrade
    // silently and the GUI continues without a tray icon —
    // KDE Plasma expects this for non-graphical sessions.
    let mut tray_handle_opt = None;
    let mut tray_rx_opt = None;
    match bose_connect_gui::services::tray::TrayService::start() {
        Ok((handle, tray_rx)) => {
            tray_handle_opt = Some(handle);
            tray_rx_opt = Some(tray_rx);
        }
        Err(err) => {
            tracing::warn!(
                target: "tray",
                "status-notifier-item unavailable: {err}; running without tray icon",
            );
        }
    }
    let mut tray_handle_opt = tray_handle_opt;
    let tray_rx_opt = tray_rx_opt;

    // Push the initial tray snapshot to the SNI immediately. Without
    // this, the watcher's `Get` for `Status` / `IconPixmap` would
    // return our default values (Passive, grey glyph) even when the
    // model already has a live `DeviceSnapshot`. We push once now
    // and again on every battery-tick so the tray icon's status /
    // colour track the model's battery band live.
    if let Some(tray) = tray_handle_opt.as_ref() {
        let initial = bose_connect_gui::services::tray::TraySnapshot {
            connected: model.snapshot.is_some(),
            battery: model.snapshot.as_ref().map(|s| s.battery).unwrap_or(0),
            name: model
                .snapshot
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
        };
        tray.update(initial);
    }

    let model_holder: std::rc::Rc<std::cell::RefCell<AppModel>> =
        std::rc::Rc::new(std::cell::RefCell::new(model));
    let model_for_pump = model_holder.clone();
    let service_clone = _service.clone();
    let tx_for_widget_tray = tx.clone();

    // Render-trigger — a closure we can call from inside the
    // widget pump to rebuild the content tree after the model
    // changes (e.g. on `NavigateTo`). We swap the page content
    // box's only child rather than rebuilding the whole window
    // so the headerbar / sidebar / connection banner survive.
    let render_state: std::rc::Rc<std::cell::RefCell<Option<RenderHandle>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    // Widget pump — drains the `flume` channel into the model
    // and re-renders the content area whenever `current_page`
    // (or anything visible) is touched by the reducer.
    let pump_model = model_for_pump.clone();
    let pump_service = service_clone.clone();
    let pump_render = render_state.clone();
    let pump_tx = tx.clone();
    glib::MainContext::default().spawn_local(async move {
        loop {
            let msg = match rx.recv_async().await {
                Ok(m) => m,
                Err(_) => break,
            };
            // Show "connecting" (and drop the old device's data)
            // before the blocking call, which can take seconds.
            if before_call(&mut pump_model.borrow_mut(), &msg) {
                if let Some(handle) = pump_render.borrow().as_ref() {
                    handle.rebuild();
                }
            }
            // Capture the state *before* applying the message so
            // we know whether what the window shows changed.
            let prev = Shown::of(&pump_model.borrow());
            let reload_after = matches!(
                msg,
                AppMsg::Connect(_) | AppMsg::Disconnect | AppMsg::Refresh
            );
            // Messages are handled one at a time, so device calls
            // never interleave on the single connection.
            let quiet_mode = pump_model.borrow().quiet_mode;
            let service = pump_service.clone();
            let (msg, outcome) = match gtk::gio::spawn_blocking(move || {
                let outcome = call_service(&service, &msg, quiet_mode);
                (msg, outcome)
            })
            .await
            {
                Ok(pair) => pair,
                Err(_) => {
                    tracing::error!(target: "device", "device call panicked");
                    continue;
                }
            };
            // A failed setter leaves the snapshot as it was, but the
            // widget the user touched already shows the new value:
            // redraw to put it back.
            let device_failed = matches!(outcome, Outcome::Snapshot(Err(_)));
            {
                let mut guard = pump_model.borrow_mut();
                apply(&mut guard, msg, outcome);
            }
            let changed = Shown::of(&pump_model.borrow()) != prev;
            // A connection change can change which devices BlueZ
            // reports as connected.
            if reload_after {
                let _ = pump_tx.send(AppMsg::ReloadDevices);
            }
            // Redraw only when what the page shows changed: rebuilding
            // the page after every device answer made it flicker (the
            // 60 s refresh of a real device usually returns the same
            // snapshot).
            if changed || (real_device && device_failed) {
                if let Some(handle) = pump_render.borrow().as_ref() {
                    handle.rebuild();
                }
            }
        }
    });

    // Tray pump — drains the `TrayCommand` channel into the
    // same message stream as the widgets, but on a fast glib
    // poll (250 ms) so we can use the synchronous
    // `try_recv`. The tokio mpsc receiver's async methods
    // require a tokio runtime which we don't have on the
    // glib main context, so polling is the simpler option.
    if let Some(mut tray_rx) = tray_rx_opt {
        let tx_for_tray = tx_for_widget_tray.clone();
        glib::source::timeout_add_local(Duration::from_millis(250), move || {
            while let Ok(cmd) = tray_rx.try_recv() {
                let _ = tx_for_tray.send(AppMsg::TrayEvent(cmd));
            }
            glib::ControlFlow::Continue
        });
    } else {
        // Drop the unused sender so the borrow checker doesn't
        // panic on `None`.
        let _ = tx_for_widget_tray;
    }

    // BlueZ device events. Connects to the system bus and
    // subscribes to `InterfacesAdded` / `InterfacesRemoved`. The
    // mpsc is polled on a slow (500 ms) glib timer so we can stay
    // on the main thread. If BlueZ is not running,
    // `BluetoothDiscovery::connect` returns a handle with
    // `available() == false` and no events arrive.
    let (bt_handle, bt_rx) = match runtime()
        .block_on(bose_connect_gui::services::bluetooth::BluetoothDiscovery::connect())
    {
        Ok(pair) => pair,
        Err(err) => {
            tracing::warn!(target: "bluez", "discovery disabled: {err}");
            // Synthetic empty receiver: no events.
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<
                bose_connect_gui::services::bluetooth::DiscoveryEvent,
            >();
            let _ = tx;
            (
                bose_connect_gui::services::bluetooth::BluetoothDiscovery::empty(),
                rx,
            )
        }
    };
    // A device paired or removed meanwhile re-reads the list, at
    // most once per poll.
    let _bt_handle = bt_handle;
    let mut bt_rx = bt_rx;
    let tx_for_bt = tx.clone();
    glib::source::timeout_add_local(Duration::from_millis(500), move || {
        use bose_connect_gui::services::bluetooth::DiscoveryEvent;
        let mut reload = false;
        while let Ok(ev) = bt_rx.try_recv() {
            reload |= match ev {
                DiscoveryEvent::DeviceFound(d) => d.paired && d.is_bose,
                DiscoveryEvent::DeviceRemoved(_) => true,
                DiscoveryEvent::AdapterPowered(_) => false,
            };
        }
        if reload {
            let _ = tx_for_bt.send(AppMsg::ReloadDevices);
        }
        glib::ControlFlow::Continue
    });

    // Periodic battery tick — drives the sparkline redraws AND
    // fires desktop notifications when the battery crosses low /
    // critical thresholds (the KDE Plasma info-bar via libnotify).
    let tick_model = model_holder.clone();
    let tick_tx = tx.clone();
    // Use a tighter tick when running the low-battery smoke so
    // the threshold-crossing fires before the headless timer
    // shuts the binary down. The default 4 s cadence is right
    // for an interactive session; the mock drains at 250 ms in
    // the smoke path, so the GUI needs to sample at least that
    // often to keep the sparkline live.
    let tick_period_ms: u64 = if cli.low_battery_test {
        250
    } else {
        TICK_INTERVAL_MS
    };
    let tick_service = _service.clone();
    let tray_handle_for_tick = tray_handle_opt.take();
    glib::timeout_add_local(Duration::from_millis(tick_period_ms), move || {
        // Pull the live battery from the service so the mock's
        // background drain actually reaches the GUI. The model
        // only knows about the last *refreshed* battery; the
        // mock can change faster than the user can press
        // Refresh, and we want the threshold-crossing to fire
        // without manual interaction.
        // The real transport is read through `AppMsg::Refresh`
        // instead (see below): a device call here would block the
        // main thread.
        let live_battery = if real_device {
            None
        } else {
            tick_service
                .refresh()
                .ok()
                .map(|s| (s.battery, s.name.clone()))
        };
        let prev = {
            let g = tick_model.borrow();
            g.history.last().copied().unwrap_or(100)
        };
        let _ = tick_tx.send(AppMsg::BatteryTick(0));
        // Mirror the latest battery into the history.
        let now = live_battery
            .as_ref()
            .map(|(b, _)| *b)
            .or_else(|| tick_model.borrow().snapshot.as_ref().map(|s| s.battery));
        if let Some(b) = now {
            {
                let mut g = tick_model.borrow_mut();
                g.history.push(b);
                if g.history.len() > 32 {
                    g.history.remove(0);
                }
            }
            // Fire notifications on threshold crossings.
            let prev_band = band(prev);
            let now_band = band(b);
            if prev_band != now_band && now_band >= 0 {
                let level = if now_band <= 1 {
                    NotificationLevel::Critical
                } else {
                    NotificationLevel::Warning
                };
                let body = match now_band {
                    0 => bose_connect_gui::t!("notify-battery-critical"),
                    1 => bose_connect_gui::t!("notify-battery-low"),
                    _ => bose_connect_gui::t!("notify-battery-threshold"),
                };
                let title = "Bose Connect";
                tracing::info!(
                    target: "battery",
                    "cross band {prev_band} -> {now_band} (battery={b}%); notifying"
                );
                let _ = Notifications::notify(title, &body, level);
            }

            // Mirror the latest snapshot into the SNI so the
            // tray icon's `Status` / `IconPixmap` track the live
            // battery band (Active ≥ 26 %, NeedsAttention ≤ 25 %,
            // Passive when no device).
            if let Some(tray) = tray_handle_for_tick.as_ref() {
                let snap = if real_device {
                    tick_model.borrow().snapshot.clone()
                } else {
                    tick_service.refresh().ok()
                };
                tray.update(match snap {
                    Some(snap) => bose_connect_gui::services::tray::TraySnapshot {
                        connected: true,
                        battery: snap.battery,
                        name: snap.name.clone(),
                    },
                    None => bose_connect_gui::services::tray::TraySnapshot {
                        connected: false,
                        battery: 0,
                        name: String::new(),
                    },
                });
            }
        }
        glib::ControlFlow::Continue
    });

    // Re-read the real device now and then (battery, paired
    // sources), over the open connection and off the main thread.
    if real_device {
        let refresh_model = model_holder.clone();
        let refresh_tx = tx.clone();
        glib::timeout_add_local(Duration::from_millis(REAL_REFRESH_MS), move || {
            // `Refresh` re-reads the paired list too.
            let msg = if refresh_model.borrow().connection == ConnectionState::Connected {
                AppMsg::Refresh
            } else {
                AppMsg::ReloadDevices
            };
            let _ = refresh_tx.send(msg);
            glib::ControlFlow::Continue
        });
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Bose Connect")
        .default_width(780)
        .default_height(840)
        .build();

    // Register window actions for header bar and kebab menu
    let act_refresh = gtk::gio::SimpleAction::new("refresh", None);
    let tx_refresh = tx.clone();
    act_refresh.connect_activate(move |_, _| {
        let _ = tx_refresh.send(AppMsg::Refresh);
    });
    window.add_action(&act_refresh);

    let act_mybose = gtk::gio::SimpleAction::new("mybose", None);
    let tx_mybose = tx.clone();
    act_mybose.connect_activate(move |_, _| {
        let _ = tx_mybose.send(AppMsg::NavigateTo(Page::MyBose));
    });
    window.add_action(&act_mybose);

    let act_about = gtk::gio::SimpleAction::new("about", None);
    let win_about = window.clone();
    act_about.connect_activate(move |_, _| {
        bose_connect_gui::app::widgets::show_about_dialog(&win_about);
    });
    window.add_action(&act_about);

    let act_quit = gtk::gio::SimpleAction::new("quit", None);
    let app_quit = application.clone();
    act_quit.connect_activate(move |_, _| {
        app_quit.quit();
    });
    window.add_action(&act_quit);

    // Build the full widget tree on top of the seeded model.
    // We split the tree into the chrome (toolbar view + sidebar)
    // and the swappable content box so `RenderHandle::rebuild`
    // can swap pages without re-creating the headerbar / sidebar.
    let (root_widget, content_box, render_handle) =
        build_root(&application, &model_holder.borrow(), &tx, &model_holder);
    *render_state.borrow_mut() = Some(render_handle);

    window.set_content(Some(&root_widget));
    window.set_visible(true);

    // Debug keyboard shortcuts for verifying the navigation pump
    // when we can't simulate real mouse clicks. Ctrl+1/2 jump
    // straight to the corresponding page.
    let key_tx = tx.clone();
    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    key_controller.connect_key_pressed(move |_controller, _key, keycode, state| {
        if !(state == gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let page = match keycode {
            v if v == b'1' as u32 => Some(Page::Overview),
            v if v == b'2' as u32 => Some(Page::MyBose),
            _ => None,
        };
        let _ = _controller;
        if let Some(p) = page {
            tracing::info!(
                target: "debug-keys",
                "Ctrl+{} (scancode={}) → NavigateTo({})",
                char::from_u32(keycode).unwrap_or('?'),
                keycode,
                p.title()
            );
            let _ = key_tx.send(AppMsg::NavigateTo(p));
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    // Optional tree dump for smoke tests / debugging.
    if cli.dump_tree {
        let mut out = String::new();
        dump_widget_tree(&root_widget, 0, &mut out);
        println!("{}", out);
    }

    let _ = content_box;

    let _ = application;

    if cli.headless || cli.run_secs.is_some() {
        // `application` is the owned `adw::Application`; cloning
        // it bumps the underlying `glib::Object` refcount so we
        // can move the clone into the `'static` closure below
        // without dropping the only reference.
        let app_for_quit = application.clone();
        let screenshot = cli.screenshot.clone();
        let run_for = cli.run_secs.unwrap_or(0);
        // Compute the exit delay up-front so the closure can be
        // moved onto the glib main context (where calling
        // `app.quit()` is safe). The smoke test cases need at
        // least 7 s when the low-battery mock is doing its
        // 250 ms tick so the threshold-crossing fires before
        // the binary shuts down.
        let delay = if cli.low_battery_test {
            std::time::Duration::from_secs(run_for.max(7))
        } else if run_for > 0 {
            std::time::Duration::from_secs(run_for)
        } else if screenshot.is_some() {
            std::time::Duration::from_millis(2500)
        } else if cli.tour {
            std::time::Duration::from_millis(5000)
        } else {
            std::time::Duration::from_millis(700)
        };
        let headless = cli.headless;

        // The screenshot must run on the glib main thread — its
        // widget tree isn't `Send`. We schedule a separate
        // one-shot for the screenshot path and a one-shot for the
        // exit / smoke marker so each runs on the right context.
        if let Some(path) = screenshot.clone() {
            let widget = root_widget.clone();
            let path_clone = std::path::PathBuf::from(path);
            glib::source::timeout_add_local_once(Duration::from_millis(1500), move || {
                capture_widget_ppm(&widget, &path_clone);
            });
        }

        // `--tour` cycles through every sidebar page once and
        // dumps the widget tree after each step. This proves the
        // `AppMsg::NavigateTo` plumbing actually swaps the
        // content area, even on a headless host with no display
        // server. We pick up the sender that lives in
        // `activate()` (cloned into `tx_for_widget_tray`); in
        // headless mode the tray pump never fires so `tx_for_tray`
        // is dropped and we own the original `tx`.
        if cli.tour {
            let tour_tx = tx.clone();
            let tour_widget = root_widget.clone();
            let mut iter = Page::ALL.iter().copied().cycle();
            let step_count = std::rc::Rc::new(std::cell::Cell::new(0usize));
            glib::source::timeout_add_local(Duration::from_millis(1500), move || {
                let next = iter.next().unwrap_or(Page::Overview);
                let _ = tour_tx.send(AppMsg::NavigateTo(next));
                let n = step_count.get() + 1;
                step_count.set(n);
                let widget_clone = tour_widget.clone();
                let page_label = next.title().to_string();
                glib::source::timeout_add_local_once(Duration::from_millis(500), move || {
                    tracing::info!(
                        target: "tour",
                        "tour step {n}: navigated to {page_label}; widget tree:"
                    );
                    let mut out = String::new();
                    dump_widget_tree(&widget_clone, 0, &mut out);
                    for line in out.lines() {
                        tracing::info!(target: "tour-tree", "{line}");
                    }
                });
                if n >= Page::ALL.len() {
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            });
        }

        // Exit / smoke marker — also on the glib main thread so
        // we can call `app.quit()`. After `app.quit()` returns,
        // the GTK main loop ends but the runtime's worker
        // threads and the `spawn_local` pump may still be alive
        // long enough to keep the process around; we follow up
        // with a hard `exit(0)` so the smoke test is reliable
        // on CI.
        glib::source::timeout_add_local_once(delay, move || {
            if screenshot.is_some() {
                tracing::info!("screenshot scheduled; see stdout for the path it landed at");
            }
            if headless {
                tracing::info!("smoke test: window painted, exiting");
            } else {
                tracing::info!("smoke test: run_secs elapsed, exiting");
            }
            let _ = std::fs::write("/tmp/bose-connect-gui.smoke", b"ok\n");
            app_for_quit.quit();
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::process::exit(0);
        });
    }
}

use std::time::Duration;

/// Bucket battery levels for notification thresholds.
/// Returns `-1` when unknown, `0` for critical (≤5%), `1` for low
/// (≤25%), and `2` for OK.
fn band(battery: u8) -> i32 {
    match battery {
        0..=5 => 0,
        6..=25 => 1,
        _ => 2,
    }
}

/// The device to connect to at startup: the last one, while it is
/// still paired and BlueZ has it connected, else the first paired
/// Bose device BlueZ has connected. A device that is off is not
/// tried, so the window opens on "connect to a device" at once.
fn startup_device(last: Option<&str>, devices: &Option<Vec<PairedBose>>) -> Option<String> {
    let connected: Vec<&PairedBose> = devices.iter().flatten().filter(|d| d.connected).collect();
    last.and_then(|last| {
        connected
            .iter()
            .find(|d| d.address.eq_ignore_ascii_case(last))
    })
    .or_else(|| connected.first())
    .map(|d| d.address.clone())
}

/// Update the model before the blocking call behind `msg`; true when
/// the window must be redrawn. Switching devices drops the old one's
/// data at once, so it is never shown as the new device's.
fn before_call(model: &mut AppModel, msg: &AppMsg) -> bool {
    match msg {
        AppMsg::Connect(address) => {
            model.snapshot = None;
            model.history.clear();
            model.connection = ConnectionState::Connecting(address.clone());
            true
        }
        AppMsg::JoinParty { .. } | AppMsg::LeaveParty(_) | AppMsg::SetPartyMode(_) => {
            model.party_busy = true;
            model.party_error = None;
            true
        }
        _ => false,
    }
}

/// What the window shows; a change triggers a full redraw.
#[derive(PartialEq)]
struct Shown {
    page: Page,
    snapshot: Option<DeviceSnapshot>,
    connection: ConnectionState,
    devices: Option<Vec<PairedBose>>,
    party_busy: bool,
    party_error: Option<String>,
}

impl Shown {
    fn of(model: &AppModel) -> Self {
        Self {
            page: model.current_page,
            snapshot: model.snapshot.clone(),
            connection: model.connection.clone(),
            devices: model.host_devices.clone(),
            party_busy: model.party_busy,
            party_error: model.party_error.clone(),
        }
    }
}

/// Dump the GTK widget tree rooted at `widget` to a `String`.
fn dump_widget_tree(widget: &gtk::Widget, depth: usize, out: &mut String) {
    use std::fmt::Write as _;
    let indent = "  ".repeat(depth);
    let type_name = widget.type_().name().to_string();
    let name = widget.widget_name().to_string();
    let classes: Vec<String> = widget
        .css_classes()
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    let classes_str = classes.join(",");
    let label_summary = if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
        format!("  «{}»", label.text())
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "{indent}{type_name}#{name} [{classes_str}]{label_summary}",
    );
    let mut cursor = widget.first_child();
    while let Some(child) = cursor {
        dump_widget_tree(&child, depth + 1, out);
        match child.next_sibling() {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
}

/// Result of the blocking device call behind one [`AppMsg`].
enum Outcome {
    /// The message does not talk to the device.
    None,
    Snapshot(anyhow::Result<Box<DeviceSnapshot>>),
    Unit(anyhow::Result<()>),
    Devices(anyhow::Result<Vec<PairedBose>>),
    /// A party-mode change, and the restart of this computer's audio
    /// stream that follows a successful one.
    Party {
        result: anyhow::Result<Box<DeviceSnapshot>>,
        restart: Option<anyhow::Result<bool>>,
    },
    Text(anyhow::Result<String>),
}

impl Outcome {
    fn snapshot(self) -> anyhow::Result<DeviceSnapshot> {
        match self {
            Outcome::Snapshot(r) => r.map(|b| *b),
            Outcome::Unit(r) => r.and_then(|()| anyhow::bail!("no snapshot returned")),
            Outcome::Party { result, .. } => result.map(|b| *b),
            Outcome::Devices(_) | Outcome::Text(_) | Outcome::None => {
                anyhow::bail!("no device call was made")
            }
        }
    }

    fn unit(self) -> anyhow::Result<()> {
        match self {
            Outcome::Snapshot(r) => r.map(|_| ()),
            Outcome::Unit(r) => r,
            Outcome::Party { result, .. } => result.map(|_| ()),
            Outcome::Devices(_) | Outcome::Text(_) | Outcome::None => {
                anyhow::bail!("no device call was made")
            }
        }
    }
}

/// Run the device call behind `msg`. It blocks on socket I/O with
/// the real service, so the pump runs it with
/// `gio::spawn_blocking`, never on the GTK main thread.
fn call_service(service: &Arc<dyn DeviceService>, msg: &AppMsg, quiet_mode: bool) -> Outcome {
    use bose_connect_gui::app::model::AppMsg as M;
    let snap = |r: anyhow::Result<DeviceSnapshot>| Outcome::Snapshot(r.map(Box::new));
    match msg {
        M::Connect(address) => snap(service.connect(address)),
        M::Disconnect => {
            service.disconnect();
            Outcome::None
        }
        M::ReloadDevices => Outcome::Devices(service.known_devices()),
        M::Refresh => snap(service.refresh()),
        M::SetNoiseCancelling(level) => snap(service.set_noise_cancelling(*level)),
        M::SetAutoOff(min) => snap(service.set_auto_off(*min)),
        M::SetLanguage {
            language,
            voice_prompts,
        } => snap(service.set_language(*language, *voice_prompts)),
        M::SetVoicePrompts(on) => snap(service.set_voice_prompts(*on)),
        M::SetPairing(on) => snap(service.set_pairing(*on)),
        M::SetName(name) => snap(service.set_name(name)),
        M::SetVolume(level) => snap(service.set_volume(*level)),
        M::SendMediaKey(key) => Outcome::Unit(service.send_media_key(*key)),
        M::SetSelfVoice(level) => snap(service.set_self_voice(*level)),
        M::PairedConnect(addr) => snap(service.connect_device(*addr)),
        M::PairedDisconnect(addr) => snap(service.disconnect_device(*addr)),
        M::PairedRemove(addr) => snap(service.remove_device(*addr)),
        M::JoinParty { peer, source } => party(service, service.join_party(*peer, *source)),
        M::LeaveParty(peer) => party(service, service.leave_party(*peer)),
        M::SetPartyMode(mode) => party(service, service.set_party_mode(*mode)),
        M::Use44100 => Outcome::Text(
            bose_connect_gui::services::audio::use_44100_for_bluetooth()
                .map(|path| path.display().to_string()),
        ),
        M::ApplyProfile(profile) => {
            // Apply each setting in turn, in the original C CLI's
            // order; a failed one does not stop the others.
            let p = bose_connect_gui::services::state::Profiles::resolve(*profile);
            let mut last = None;
            if let Some(nc) = p.noise_cancelling {
                last = Some(service.set_noise_cancelling(nc));
            }
            if let Some(voice) = p.voice_prompts {
                last = Some(service.set_voice_prompts(voice));
            }
            if let Some(auto) = p.auto_off {
                last = Some(service.set_auto_off(auto));
            }
            last.map_or(Outcome::None, snap)
        }
        // `quiet_mode` is the state *before* the toggle.
        M::ToggleQuietMode if !quiet_mode => {
            let _ = service.set_noise_cancelling(bose_connect::NoiseCancelling::High);
            let _ = service.set_voice_prompts(false);
            snap(service.set_auto_off(bose_connect::AutoOff::Never))
        }
        _ => Outcome::None,
    }
}

/// After a party-mode change the speaker stays silent until this
/// computer restarts its audio stream; give it a moment first (it
/// announces the new mode).
fn party(service: &Arc<dyn DeviceService>, result: anyhow::Result<DeviceSnapshot>) -> Outcome {
    let restart = result.is_ok().then(|| {
        std::thread::sleep(PARTY_RESTART_DELAY);
        service.restart_audio_stream()
    });
    Outcome::Party {
        result: result.map(Box::new),
        restart,
    }
}

/// Wait between a party-mode change and the audio-stream restart.
const PARTY_RESTART_DELAY: Duration = Duration::from_secs(2);

/// Log the audio-stream restart that follows a party-mode change.
fn log_restart(model: &mut AppModel, outcome: &Outcome) {
    if let Outcome::Party {
        restart: Some(restart),
        ..
    } = outcome
    {
        match restart {
            Ok(true) => model.log_info("restarted the audio stream from this computer"),
            Ok(false) => {}
            Err(e) => model.log_error(format!(
                "restart the audio stream: {e}; pause and play the music"
            )),
        }
    }
}

fn apply(model: &mut AppModel, msg: AppMsg, outcome: Outcome) {
    use bose_connect_gui::app::model::AppMsg as M;
    match msg {
        M::Connect(address) => match outcome.snapshot() {
            Ok(snap) => {
                tracing::info!(target: "device", "connected to {address} ({})", snap.name);
                model.log_success(format!("connected to {}", snap.name));
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
                // The simulator must not replace the real device
                // the next launch reconnects to.
                if !model.simulator {
                    model.persisted.last_address = Some(address);
                    model.persisted.save();
                }
            }
            Err(e) => {
                tracing::warn!(target: "device", "connect {address}: {e}");
                model.log_error(format!("connect {address}: {e}"));
                model.snapshot = None;
                model.connection = ConnectionState::Error(e.to_string());
            }
        },
        M::Disconnect => {
            if let Some(snap) = model.snapshot.take() {
                tracing::info!(target: "device", "disconnected from {}", snap.name);
                model.log_info(format!("disconnected from {}", snap.name));
            }
            model.history.clear();
            model.connection = ConnectionState::NotStarted;
        }
        M::ReloadDevices => {
            if let Outcome::Devices(result) = outcome {
                match result {
                    Ok(devices) => model.host_devices = Some(devices),
                    Err(e) => {
                        tracing::warn!(target: "bluez", "paired devices: {e}");
                        model.host_devices = None;
                    }
                }
            }
        }
        M::Refresh => match outcome.snapshot() {
            Ok(snap) => {
                tracing::info!(target: "device", "refreshed {}", snap.name);
                model.snapshot = Some(snap.clone());
                model.connection = ConnectionState::Connected;
                model.log_success(format!("refreshed: {}", snap.name));
            }
            Err(e) => model.log_error(format!("refresh: {e}")),
        },
        M::SetNoiseCancelling(_) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success("set noise cancelling");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set NC: {e}")),
        },
        M::SetAutoOff(_) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success("set auto-off");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set auto-off: {e}")),
        },
        M::SetLanguage { .. } => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success("set language");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set lang: {e}")),
        },
        M::SetVoicePrompts(_) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success("set voice prompts");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set voice prompts: {e}")),
        },
        M::SetPairing(_) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success("toggle pairing");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set pairing: {e}")),
        },
        M::SetName(name) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success(format!("renamed to {name}"));
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set name: {e}")),
        },
        M::SetVolume(level) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success(format!("volume set to {level} / 75"));
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set volume: {e}")),
        },
        M::SendMediaKey(key) => match outcome.unit() {
            Ok(()) => {
                model.log_success(format!("media key sent: {key:?}"));
            }
            Err(e) => model.log_error(format!("media key: {e}")),
        },
        M::SetSelfVoice(_) => match outcome.snapshot() {
            Ok(snap) => {
                model.log_success("set self-voice");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set self-voice: {e}")),
        },
        M::PairedConnect(addr) => match outcome.snapshot() {
            Ok(snap) => {
                let formatted = bose_connect_gui::i18n::format_address(addr);
                model.log_success(format!("connecting source {formatted}"));
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("connect device: {e}")),
        },
        M::PairedDisconnect(addr) => match outcome.snapshot() {
            Ok(snap) => {
                let formatted = bose_connect_gui::i18n::format_address(addr);
                model.log_success(format!("disconnecting source {formatted}"));
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("disconnect device: {e}")),
        },
        M::PairedRemove(addr) => match outcome.snapshot() {
            Ok(snap) => {
                let formatted = bose_connect_gui::i18n::format_address(addr);
                model.log_success(format!("removed source {formatted}"));
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("remove device: {e}")),
        },
        M::JoinParty { peer, .. } => {
            model.party_busy = false;
            log_restart(model, &outcome);
            let formatted = bose_connect_gui::i18n::format_address(peer);
            match outcome.snapshot() {
                Ok(snap) => {
                    model.log_success(format!("joined {formatted}: party mode"));
                    model.snapshot = Some(snap);
                    if !model.simulator {
                        model.persisted.party_peer = Some(formatted);
                        model.persisted.save();
                    }
                }
                Err(e) => {
                    model.party_error = Some(e.to_string());
                    model.log_error(format!("join {formatted}: {e}"));
                }
            }
        }
        M::LeaveParty(peer) => {
            model.party_busy = false;
            log_restart(model, &outcome);
            let formatted = bose_connect_gui::i18n::format_address(peer);
            match outcome.snapshot() {
                Ok(snap) => {
                    model.log_success(format!("separated {formatted}"));
                    model.snapshot = Some(snap);
                    if !model.simulator {
                        model.persisted.party_peer = None;
                        model.persisted.save();
                    }
                }
                Err(e) => {
                    model.party_error = Some(e.to_string());
                    model.log_error(format!("separate {formatted}: {e}"));
                }
            }
        }
        M::SetPartyMode(mode) => {
            model.party_busy = false;
            log_restart(model, &outcome);
            match outcome.snapshot() {
                Ok(snap) => {
                    model.log_success(format!("party mode: {mode:?}"));
                    model.snapshot = Some(snap);
                }
                Err(e) => {
                    model.party_error = Some(e.to_string());
                    model.log_error(format!("party mode {mode:?}: {e}"));
                }
            }
        }
        M::Use44100 => match outcome {
            Outcome::Text(Ok(path)) => model.log_success(format!(
                "wrote {path}; Bluetooth audio links opened from now on use 44.1 kHz"
            )),
            Outcome::Text(Err(e)) => model.log_error(format!("44.1 kHz: {e}")),
            _ => {}
        },
        M::ApplyProfile(profile) => {
            model.log_info(format!("applying profile {}", profile.label()));
            // Apply each setting sequentially. We forward through
            // the same `apply` function so the order matches the
            // original C CLI.
            if let Ok(snap) = outcome.snapshot() {
                model.snapshot = Some(snap);
            }
            model.persisted.active_profile = Some(profile);
            model.persisted.save();
        }
        M::ToggleQuietMode => {
            model.quiet_mode = !model.quiet_mode;
            model.log_info(format!(
                "quiet mode {}",
                if model.quiet_mode { "on" } else { "off" }
            ));
            if let Ok(snap) = outcome.snapshot() {
                model.snapshot = Some(snap);
            }
        }
        M::Acknowledge => model.log.clear(),
        M::BatteryTick(_) => {
            tracing::trace!("battery tick");
        }
        M::NavigateTo(page) if model.current_page != page => {
            // Only mark the page as dirty when it actually
            // changes, so a duplicate NavigateTo doesn't trigger
            // a no-op render.
            model.current_page = page;
            model.persisted.last_page = Some(page.key().to_string());
            model.persisted.save();
            model.log_info(format!("navigated to {}", page.title()));
        }
        _ => {}
    }
}

/// Best-effort screenshot: render the widget tree onto an
/// off-screen `Snapshot`, paint the resulting `gsk::RenderNode`
/// onto a `cairo::ImageSurface`, then dump the premultiplied
/// ARGB32 bytes to disk as PPM.
///
/// Why PPM and not PNG? `cairo-rs 0.22` does not re-export
/// `Surface::write_to_png` for every feature combination, and the
/// `png` crate's public API churns between releases; PPM is plain
/// enough to write by hand (header line + raw RGB), avoiding yet
/// another moving target. ImageMagick / `display` / `gimp` open
/// PPM files out of the box.
///
/// Triggered only by `--screenshot PATH`; falls back silently to a
/// log line on any error.
fn capture_widget_ppm(widget: &gtk::Widget, path: &std::path::Path) {
    use std::fs;
    use std::io::Write as _;

    let width = (widget.width().max(780) as u32).clamp(780, 1920);
    let height = (widget.height().max(840) as u32).clamp(840, 3000);

    // Off-screen image surface and cairo context.
    let surface =
        match cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "screenshot", "cairo surface: {e:?}");
                return;
            }
        };
    let cr = match cairo::Context::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(target: "screenshot", "cairo context: {e:?}");
            return;
        }
    };

    // Fill the background with the libadwaita canvas color
    // so transparent pixels don't survive.
    if adw::StyleManager::default().is_dark() {
        cr.set_source_rgba(0.14, 0.14, 0.14, 1.0);
    } else {
        cr.set_source_rgba(0.96, 0.96, 0.96, 1.0);
    }
    cr.paint().ok();

    // Render the widget into a fresh `Snapshot` and paint the
    // resulting `RenderNode` onto the cairo surface.
    let snapshot = gtk::Snapshot::new();
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    gtk::gdk::prelude::PaintableExt::snapshot(&paintable, &snapshot, width as f64, height as f64);
    if let Some(node) = snapshot.to_node() {
        node.draw(&cr);
    } else {
        tracing::warn!(target: "screenshot", "snapshot had no RenderNode");
    }
    drop(cr);

    // Read the raw premultiplied ARGB32 buffer.
    let raw = match surface.take_data() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(target: "screenshot", "could not read surface data: {e:?}");
            return;
        }
    };
    let raw_pixels: &[u8] = raw.as_ref();
    let row_bytes = width as usize * 4;
    if raw_pixels.len() < row_bytes * height as usize {
        tracing::warn!(target: "screenshot", "raw pixel buffer too small");
        return;
    }

    let mut file = match fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(target: "screenshot", "open {}: {e}", path.display());
            return;
        }
    };
    // PPM header (P6 = binary RGB; cairo gives us BGRA premultiplied,
    // so after un-premultiplying per row we emit BGR as PPM expects).
    let header = format!("P6\n{width} {height}\n255\n");
    if let Err(e) = file.write_all(header.as_bytes()) {
        tracing::warn!(target: "screenshot", "write header: {e}");
        return;
    }
    let row_pixels = row_bytes;
    let mut buf: Vec<u8> = Vec::with_capacity(width as usize * 3 * height as usize);
    for chunk in raw_pixels.chunks_exact(row_pixels).take(height as usize) {
        for px in chunk.chunks_exact(4) {
            let b = px[0];
            let g = px[1];
            let r = px[2];
            let a = px[3];
            // Un-premultiply if needed; PPM ignores alpha so the
            // fastest path for fully opaque / transparent pixels
            // is to emit directly.
            let (rr, gg, bb) = if a == 0 || a == 255 {
                (r, g, b)
            } else {
                let inv = 255u32 * 255 / a as u32;
                let rr = ((r as u32 * inv) / 255).min(255) as u8;
                let gg = ((g as u32 * inv) / 255).min(255) as u8;
                let bb = ((b as u32 * inv) / 255).min(255) as u8;
                (rr, gg, bb)
            };
            // PPM `P6` writes RGB in big-endian: order is R, G, B.
            // (Many viewers also accept BGR because PPM is barely
            // a standard, but `display` and ImageMagick parse both.)
            buf.extend_from_slice(&[rr, gg, bb]);
        }
    }
    if let Err(e) = file.write_all(&buf) {
        tracing::warn!(target: "screenshot", "write body: {e}");
        return;
    }
    tracing::info!(
        target: "screenshot",
        "wrote screenshot to {} ({} bytes, PPM P6)",
        path.display(),
        buf.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(address: &str, connected: bool) -> PairedBose {
        PairedBose {
            address: address.to_owned(),
            name: address.to_owned(),
            connected,
            product_id: 0x4066,
        }
    }

    #[test]
    fn startup_prefers_the_last_device_while_it_is_connected() {
        let devices = Some(vec![
            device("02:00:00:00:00:01", true),
            device("02:00:00:00:00:02", true),
        ]);
        assert_eq!(
            startup_device(Some("02:00:00:00:00:02"), &devices).as_deref(),
            Some("02:00:00:00:00:02")
        );
        // The last device is off: take a connected one.
        let devices = Some(vec![
            device("02:00:00:00:00:01", true),
            device("02:00:00:00:00:02", false),
        ]);
        assert_eq!(
            startup_device(Some("02:00:00:00:00:02"), &devices).as_deref(),
            Some("02:00:00:00:00:01")
        );
    }

    #[test]
    fn startup_tries_nothing_without_a_connected_device() {
        let devices = Some(vec![device("02:00:00:00:00:01", false)]);
        assert_eq!(startup_device(Some("02:00:00:00:00:01"), &devices), None);
        assert_eq!(startup_device(None, &Some(Vec::new())), None);
        assert_eq!(startup_device(Some("02:00:00:00:00:01"), &None), None);
    }
}
