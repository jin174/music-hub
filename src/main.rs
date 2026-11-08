#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod config;
pub mod db;
pub mod lastfm;
pub mod monitor;
pub mod ncm;
pub mod normalize;
pub mod scrobbler;
pub mod server;
pub mod tray;
pub mod widget;

use app::{App, WidgetCommand};
use config::Config;
use db::DB;
use std::sync::Arc;
use tokio::sync::mpsc;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Single-instance check via Win32 named Mutex
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Global\\MusicHub_SingleInstance_Mutex"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let args: Vec<String> = std::env::args().collect();
            let wants_mini = args.iter().any(|a| a == "--mini");
            if wants_mini {
                let _ = reqwest::Client::new()
                    .post("http://127.0.0.1:8765/api/open_mini")
                    .send()
                    .await;
            } else {
                let _ = open::that("http://127.0.0.1:8765");
            }
            return Ok(());
        }
    }

    // 2. Parse CLI flags
    let args: Vec<String> = std::env::args().collect();
    let mut custom_port = None;
    let mut start_mini = false;
    let mut open_browser = true;

    for arg in &args[1..] {
        if arg == "--mini" {
            start_mini = true;
        } else if arg == "--no-browser" {
            open_browser = false;
        } else if arg == "--console" {
            open_browser = true;
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

    // 4. Create widget communication channel and spawn UI threads
    let (widget_tx, widget_rx) = mpsc::unbounded_channel::<WidgetCommand>();
    *app.widget_tx.write().await = Some(widget_tx);

    // Spawn native Dynamic Island widget thread
    widget::spawn_widget_thread(app.clone(), widget_rx, start_mini);

    // Spawn system tray thread
    tray::spawn_tray_thread(app.clone());

    // 5. Start background workers
    app.scrobbler.clone().spawn_worker();
    app.monitor.clone().spawn_worker();

    // 6. Open browser console if requested and not in mini-only mode
    if open_browser && !start_mini {
        let base_url = format!("http://127.0.0.1:{}", port);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = open::that(base_url);
        });
    }

    // 7. Start Axum HTTP server (blocks main tokio task)
    server::run_server(app, port).await?;

    Ok(())
}
