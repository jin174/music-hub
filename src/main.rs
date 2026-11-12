#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod config;
pub mod db;
pub mod lastfm;
pub mod monitor;
pub mod ncm;
pub mod normalize;
pub mod poster;
pub mod scrobbler;
pub mod server;
pub mod tray;
pub mod ui;
pub mod widget;

use app::{App, ConsoleCommand, WidgetCommand};
use config::Config;
use db::DB;
use std::sync::Arc;
use tokio::sync::mpsc;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _rt_guard = rt.enter();

    // 1. Single-instance check via Win32 named Mutex
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Global\\MusicHub_SingleInstance_Mutex"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let args: Vec<String> = std::env::args().collect();
            let wants_mini = args.iter().any(|a| a == "--mini");
            let cfg = Config::load_or_create(None);
            let port = cfg.get_data().port;
            let endpoint = if wants_mini {
                format!("http://127.0.0.1:{}/api/open_mini", port)
            } else {
                format!("http://127.0.0.1:{}/api/open_console", port)
            };
            let _ = rt.block_on(async {
                reqwest::Client::new()
                    .post(&endpoint)
                    .timeout(std::time::Duration::from_millis(800))
                    .send()
                    .await
            });
            return Ok(());
        }
    }

    // 2. Parse CLI flags
    let args: Vec<String> = std::env::args().collect();
    let mut custom_port = None;
    let mut start_mini = false;
    let mut open_console = true;

    for arg in &args[1..] {
        if arg == "--mini" {
            start_mini = true;
        } else if arg == "--no-browser" || arg == "--no-console" {
            open_console = false;
        } else if arg == "--console" {
            open_console = true;
        } else if let Some(p) = arg.strip_prefix("--port=") {
            if let Ok(val) = p.parse::<u16>() {
                custom_port = Some(val);
            }
        }
    }

    // 3. Initialize config and DB
    let cfg = Config::load_or_create(None);
    if let Some(p) = custom_port {
        cfg.update(|d| d.port = p);
    }
    let port = cfg.get_data().port;

    let db = DB::open(None).expect("Failed to open SQLite hub.db");
    let app = Arc::new(App::new(cfg, db));

    // 4. Create widget & console communication channels
    let (widget_tx, widget_rx) = mpsc::unbounded_channel::<WidgetCommand>();
    *app.widget_tx.write().unwrap() = Some(widget_tx);

    let (console_tx, console_rx) = std::sync::mpsc::channel::<ConsoleCommand>();
    *app.console_tx.lock().unwrap() = Some(console_tx);

    // Spawn native Dynamic Island widget thread
    widget::spawn_widget_thread(app.clone(), widget_rx, start_mini);

    // Spawn system tray thread
    tray::spawn_tray_thread(app.clone());

    // 5. Start background workers on tokio runtime
    app.scrobbler.clone().spawn_worker();
    app.monitor.clone().spawn_worker();

    // 6. Start lightweight OAuth callback & single-instance IPC listener in background
    let app_srv = app.clone();
    rt.spawn(async move {
        let _ = server::run_server(app_srv, port).await;
    });

    // 7. Run GPUI native console loop on main thread (destroys window on close, reopens on tray/IPC signal)
    ui::run_console_loop(app, console_rx, open_console && !start_mini);

    Ok(())
}
