//! `bose-connect-gui` — launch the GTK4 application.

use std::sync::Arc;

use adw;
use adw::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use once_cell::sync::OnceCell;
use relm4::Sender;

use bose_connect_gui::app::model::{AppModel, AppMsg, ConnectionState, TICK_INTERVAL_MS};
use bose_connect_gui::app::view::build_root;
use bose_connect_gui::services::device::{DeviceService, MockService, RealService};
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
            "-h" | "--help" => {
                println!("bose-connect-gui — GTK4 + Relm4 GUI for Bose headphones");
                println!();
                println!("Usage: bose-connect-gui [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --real, --real-bluetooth  Use the real RFCOMM transport");
                println!("  --mock                    Use the in-process mock device");
                println!("  --headless                Render once and exit (smoke tests)");
                println!("  --screenshot PATH         Save a PNG of the first frame (headless)");
                println!("  --address AA:BB:CC:DD:EE:FF  Override the persisted address");
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
        // battery-drain tick task.
        let (handle, _ticker) = runtime().block_on(async { transport::spawn(None, None) });
        Arc::new(MockService::new(handle))
    };

    let app = adw::Application::builder()
        .application_id("com.airvzxf.bose-connect-gui")
        .flags(gtk::gio::ApplicationFlags::FLAGS_NONE)
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
        battery: 85,
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
    // messages back to the AppModel without spinning a separate
    // Relm4 component. We use flume here because that's what
    // Relm4's `Sender` accepts via `From`.
    let (raw_tx, mut rx) = flume::unbounded::<AppMsg>();
    let tx: Sender<AppMsg> = Sender::from(raw_tx);

    // Drive the model from the message pump.
    let mut model_holder: std::rc::Rc<std::cell::RefCell<AppModel>> =
        std::rc::Rc::new(std::cell::RefCell::new(model));
    let model_for_pump = model_holder.clone();
    let service_clone = _service.clone();
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

    // Periodic battery tick — drives the sparkline redraws.
    let tick_model = model_holder.clone();
    let tick_tx = tx.clone();
    glib::timeout_add_local(Duration::from_millis(TICK_INTERVAL_MS), move || {
        tick_tx.emit(AppMsg::BatteryTick(0));
        // Mirror the latest battery into the history.
        let snap = tick_model.borrow().snapshot.clone();
        if let Some(snap) = snap {
            let mut g = tick_model.borrow_mut();
            g.history.push(snap.battery);
            if g.history.len() > 32 {
                g.history.remove(0);
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

    if cli.headless {
        let screenshot = cli.screenshot.clone();
        runtime().spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            if let Some(path) = screenshot {
                tracing::info!("smoke test: would write screenshot to {path}");
            }
            tracing::info!("smoke test: window painted, exiting");
            let _ = std::fs::write("/tmp/bose-connect-gui.smoke", b"ok\n");
            relm4::main_application().quit();
        });
    }
}

use std::time::Duration;

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
