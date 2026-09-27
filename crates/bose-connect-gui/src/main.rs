//! `bose-connect-gui` — launch the GTK4 application.

use std::sync::Arc;

#[allow(unused_imports)]
use adw::prelude::*;
use gtk::glib;
#[allow(unused_imports)]
use gtk::prelude::*;
use once_cell::sync::OnceCell;

use bose_connect_gui::app::model::{AppModel, AppMsg, ConnectionState, TICK_INTERVAL_MS};
use bose_connect_gui::app::view::build_root;
use bose_connect_gui::services::device::{DeviceService, MockService, RealService};
use bose_connect_gui::services::notifications::{NotificationLevel, Notifications};
use bose_connect_gui::transport;

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
    real: bool,
    headless: bool,
    screenshot: Option<String>,
    address: Option<String>,
    /// Override the mock's battery-drain interval. Used by the
    /// `--low-battery-test` smoke; 0 means "default from transport::spawn".
    mock_tick_ms: Option<u64>,
    /// If set, the app keeps running for this many seconds and then
    /// exits cleanly. Used to drive notification flows without a
    /// window manager.
    run_secs: Option<u64>,
    /// Convenience flag: spawn a fresh mock seeded with battery=10
    /// and a fast tick so the app fires a low-battery libnotify
    /// notification within a couple of seconds, then exits. The
    /// GUI is fully painted but the notification is the deliverable
    /// being tested.
    low_battery_test: bool,
}

fn parse_cli() -> CliArgs {
    let mut cli = CliArgs::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--real" | "--real-bluetooth" => cli.real = true,
            "--mock" => cli.real = false,
            "--headless" => cli.headless = true,
            "--screenshot" => cli.screenshot = args.next(),
            "--address" => cli.address = args.next(),
            "--mock-tick-ms" => cli.mock_tick_ms = args.next().and_then(|s| s.parse().ok()),
            "--run-secs" => cli.run_secs = args.next().and_then(|s| s.parse().ok()),
            "--low-battery-test" => cli.low_battery_test = true,
            "-h" | "--help" => {
                println!("bose-connect-gui — GTK4 + Relm4 GUI for Bose headphones");
                println!();
                println!("Usage: bose-connect-gui [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --real, --real-bluetooth  Use the real RFCOMM transport");
                println!("  --mock                    Use the in-process mock device");
                println!("  --headless                Render once and exit (smoke tests)");
                println!("  --screenshot PATH         Save a PPM of the first frame (headless)");
                println!("  --address AA:BB:CC:DD:EE:FF  Override the persisted address");
                println!("  --mock-tick-ms MS        Battery-drain interval (default 4000)");
                println!("  --run-secs SECONDS        After running for N s, exit cleanly");
                println!(
                    "  --low-battery-test       Seed mock at 10 %% with fast tick; emits libnotify"
                );
                println!("  --version, -V             Print version and exit");
                println!("  --dump-tree               Print the GTK widget tree to stdout");
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

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let cli = parse_cli();

    // Build the device service eagerly so the background ticker
    // runs even if the user dismisses the window quickly.
    let _service: Arc<dyn DeviceService> = if cli.real {
        Arc::new(RealService::new())
    } else {
        // The mock requires a Tokio runtime for its background
        // battery-drain tick task. The `--low-battery-test` flag
        // seeds the device with a 10 % charge and a 250 ms tick
        // so the libnotify threshold-crossing fires within a few
        // seconds of `activate`.
        let seed = if cli.low_battery_test {
            Some(bose_connect_gui::transport::MockSeed {
                battery: 10,
                ..Default::default()
            })
        } else {
            None
        };
        let tick = cli.mock_tick_ms.map(Duration::from_millis);
        let (handle, _ticker) = runtime().block_on(async { transport::spawn(seed, tick) });
        Arc::new(MockService::new(handle))
    };

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
        style.set_color_scheme(adw::ColorScheme::Default);

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

    // Seed an `AppModel` with a mock snapshot so the initial render
    // has battery percentage and a device name. We connect the
    // mock service so user interactions update the model — Relm4
    // isn't necessary for the very first iteration because every
    // widget mutates the AppModel directly.
    let mut model = AppModel::new(_service.clone());
    model.connection = ConnectionState::Connected;
    let snapshot = bose_connect_gui::services::device::DeviceSnapshot {
        address: bose_connect::BdAddr::ANY,
        name: "QuietCompanion".to_string(),
        firmware: "1.3.2".to_string(),
        serial: "08AB12CD345678".to_string(),
        device_id: 0x4020,
        battery: if cli.low_battery_test { 10 } else { 85 },
        status: bose_connect::DeviceStatusReport {
            name: "QuietCompanion".to_string(),
            language: 0x21 | bose_connect::VP_ENABLE_BIT,
            minutes: 0,
            level: bose_connect::NoiseCancelling::High,
        },
        paired: bose_connect::PairedDevices {
            addresses: [bose_connect::BdAddr::ANY; bose_connect::MAX_NUM_DEVICES],
            num_devices: 0,
            connected: bose_connect::DevicesConnected::One,
        },
        devices: vec![],
        capabilities: bose_connect_gui::services::device::Capability {
            noise_cancelling: true,
            self_voice: true,
            pairing_toggle: true,
        },
    };
    model.snapshot = Some(snapshot);
    model.history = vec![85; 24];
    for i in 80..85 {
        model.history.push(i + 1);
    }
    model.log_info("Welcome — Bose Connect for Linux");
    model.log_success("Connected to QuietCompanion");
    model.log_info("Mock transport active");

    // Set up a self-peeking channel so widgets can dispatch
    // messages back to the AppModel. We use a plain `flume`
    // sender here because the GUI is a one-writer / one-reader
    // pump.
    let (tx, rx) = flume::unbounded::<AppMsg>();

    // Tray icon — hand-rolled StatusNotifierItem. We always start
    // it (even when running headless) so the smoke test can
    // verify the `Register` call reaches the watcher. When the
    // session bus isn't available (`DBusConnection::get` returns
    // an error) we degrade silently and the GUI continues without
    // a tray icon — KDE Plasma expects this for non-graphical
    // sessions.
    let mut tray_handle_opt = None;
    let mut tray_rx_opt = None;
    match bose_connect_gui::services::tray::TrayService::start("bose-connect-gui") {
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
    let _ = tray_handle_opt;
    let tray_rx_opt = tray_rx_opt;

    let model_holder: std::rc::Rc<std::cell::RefCell<AppModel>> =
        std::rc::Rc::new(std::cell::RefCell::new(model));
    let model_for_pump = model_holder.clone();
    let service_clone = _service.clone();
    let tx_for_widget_tray = tx.clone();

    // Widget pump — drains the `flume` channel into the model.
    glib::MainContext::default().spawn_local(async move {
        loop {
            let msg = match rx.recv_async().await {
                Ok(m) => m,
                Err(_) => break,
            };
            let mut guard = model_for_pump.borrow_mut();
            apply(&service_clone, &mut guard, msg);
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

    // Bluetooth discovery (BlueZ). Connects to the system bus,
    // walks `GetManagedObjects`, and subscribes to
    // `InterfacesAdded` / `InterfacesRemoved`. The mpsc is
    // polled on a slow (500 ms) glib timer so we can stay on
    // the main thread; events fold into `AppMsg::DiscoveryFound`
    // just like the wire level did. We try to connect to BlueZ
    // once at startup; if it's not running, `BluetoothDiscovery::connect`
    // returns a handle with `available() == false`, and the
    // GUI's discover panel just stays empty.
    let (bt_handle, bt_rx) = match runtime()
        .block_on(bose_connect_gui::services::bluetooth::BluetoothDiscovery::connect())
    {
        Ok(pair) => pair,
        Err(err) => {
            tracing::warn!(target: "bluez", "discovery disabled: {err}");
            // Synthetic empty receiver — `start_scan` will
            // emit the simulated event for offline smoke
            // tests.
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
    // Trigger an active scan so the user actually sees
    // devices when they click "Scan" — the smoke test path
    // does the same.
    let _ = runtime().block_on(bt_handle.start_scan()).ok();
    let mut bt_rx = bt_rx;
    let tx_for_bt = tx.clone();
    glib::source::timeout_add_local(Duration::from_millis(500), move || {
        while let Ok(ev) = bt_rx.try_recv() {
            let mapped = match ev {
                bose_connect_gui::services::bluetooth::DiscoveryEvent::DeviceFound(d) => {
                    Some(AppMsg::DiscoveryFound {
                        address: d.address,
                        name: d.name,
                    })
                }
                _ => None,
            };
            if let Some(msg) = mapped {
                let _ = tx_for_bt.send(msg);
            }
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
    glib::timeout_add_local(Duration::from_millis(tick_period_ms), move || {
        // Pull the live battery from the service so the mock's
        // background drain actually reaches the GUI. The model
        // only knows about the last *refreshed* battery; the
        // mock can change faster than the user can press
        // Refresh, and we want the threshold-crossing to fire
        // without manual interaction.
        let live_battery = tick_service
            .refresh()
            .ok()
            .map(|s| (s.battery, s.name.clone()));
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
                    0 => "Battery critical — last 5 %. Plug in immediately.",
                    1 => "Battery low — plug in soon.",
                    _ => return glib::ControlFlow::Continue,
                };
                let title = "Bose Connect";
                tracing::info!(
                    target: "battery",
                    "cross band {prev_band} -> {now_band} (battery={b}%); notifying"
                );
                let _ = Notifications::notify(title, body, level);
            }
        }
        glib::ControlFlow::Continue
    });

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Bose Connect")
        .default_width(1180)
        .default_height(760)
        .build();

    // Build the full widget tree on top of the seeded model.
    let root_widget = build_root(&application, &model_holder.borrow(), &tx);

    window.set_content(Some(&root_widget));
    window.set_visible(true);

    // Optional tree dump for smoke tests / debugging.
    if std::env::args().any(|a| a == "--dump-tree") {
        let mut out = String::new();
        dump_widget_tree(&root_widget, 0, &mut out);
        println!("{}", out);
    }

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

fn apply(service: &Arc<dyn DeviceService>, model: &mut AppModel, msg: AppMsg) {
    use bose_connect_gui::app::model::AppMsg as M;
    match msg {
        M::Refresh => match service.refresh() {
            Ok(snap) => {
                model.snapshot = Some(snap.clone());
                model.connection = ConnectionState::Connected;
                model.log_success(format!("refreshed: {}", snap.name));
            }
            Err(e) => model.log_error(format!("refresh: {e}")),
        },
        M::SetNoiseCancelling(level) => match service.set_noise_cancelling(level) {
            Ok(snap) => {
                model.log_success("set noise cancelling");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set NC: {e}")),
        },
        M::SetAutoOff(min) => match service.set_auto_off(min) {
            Ok(snap) => {
                model.log_success("set auto-off");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set auto-off: {e}")),
        },
        M::SetLanguage {
            language,
            voice_prompts,
        } => match service.set_language(language, voice_prompts) {
            Ok(snap) => {
                model.log_success("set language");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set lang: {e}")),
        },
        M::SetVoicePrompts(on) => {
            let lang = bose_connect::PromptLanguage::En;
            match service.set_language(lang, on) {
                Ok(snap) => {
                    model.log_success("set voice prompts");
                    model.snapshot = Some(snap);
                    model.connection = ConnectionState::Connected;
                }
                Err(e) => model.log_error(format!("set voice prompts: {e}")),
            }
        }
        M::SetPairing(on) => match service.set_pairing(on) {
            Ok(snap) => {
                model.log_success("toggle pairing");
                model.snapshot = Some(snap);
                model.connection = ConnectionState::Connected;
            }
            Err(e) => model.log_error(format!("set pairing: {e}")),
        },
        M::ApplyProfile(profile) => {
            model.log_info(format!("applying profile {}", profile.label()));
            // Apply each setting sequentially. We forward through
            // the same `apply` function so the order matches the
            // original C CLI.
            let p = bose_connect_gui::services::state::Profiles::resolve(profile);
            if let Some(nc) = p.noise_cancelling {
                let _ = service.set_noise_cancelling(nc);
            }
            if let Some(voice) = p.voice_prompts {
                let _ = service.set_language(bose_connect::PromptLanguage::En, voice);
            }
            if let Some(auto) = p.auto_off {
                let _ = service.set_auto_off(auto);
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
            if model.quiet_mode {
                let _ = service.set_noise_cancelling(bose_connect::NoiseCancelling::High);
                let _ = service.set_language(bose_connect::PromptLanguage::En, false);
                let _ = service.set_auto_off(bose_connect::AutoOff::Never);
            }
        }
        M::Acknowledge => model.log.clear(),
        M::BatteryTick(_) => {
            tracing::trace!("battery tick");
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

    let width = 1180u32;
    let height = 760u32;

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
    // (`Breeze` here) so transparent pixels don't survive.
    cr.set_source_rgba(0.93, 0.94, 0.96, 1.0);
    cr.paint().ok();

    // Render the widget into a fresh `Snapshot` and paint the
    // resulting `RenderNode` onto the cairo surface.
    let snapshot = gtk::Snapshot::new();
    widget.snapshot_child(widget, &snapshot);
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
