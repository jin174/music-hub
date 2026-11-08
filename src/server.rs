use crate::app::App;
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

const EMBEDDED_INDEX_HTML: &str = include_str!("../web/index.html");
const EMBEDDED_MINI_HTML: &str = include_str!("../web/mini.html");

fn get_web_file(filename: &str) -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let external_path = exe_dir.join("web").join(filename);
    if external_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&external_path) {
            return content;
        }
    }
    match filename {
        "index.html" => EMBEDDED_INDEX_HTML.to_string(),
        "mini.html" => EMBEDDED_MINI_HTML.to_string(),
        _ => String::new(),
    }
}

pub fn make_router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(handle_index))
        .route("/index.html", get(handle_index))
        .route("/mini", get(handle_mini))
        .route("/mini.html", get(handle_mini))
        .route("/api/state", get(handle_state))
        .route("/api/recent", get(handle_recent))
        .route("/api/stats", get(handle_stats))
        .route("/api/radar", get(handle_radar))
        .route("/api/roam", get(handle_roam))
        .route("/api/ncm/match", get(handle_ncm_match))
        .route("/api/ncm/song", get(handle_ncm_song))
        .route("/api/recs/stats", get(handle_recs_stats))
        .route("/api/feedback", get(handle_get_feedback).post(handle_post_feedback))
        .route("/api/export/csv", get(handle_export_csv))
        .route("/api/export/backup", get(handle_export_backup))
        .route("/api/auth/start", get(handle_auth_start))
        .route("/callback", get(handle_callback))
        .route("/api/play", post(handle_play))
        .route("/api/play_artist", post(handle_play_artist))
        .route("/api/config", post(handle_post_config))
        .route("/api/play/delete", post(handle_delete_play))
        .route("/api/play/clear_skip", post(handle_clear_skip))
        .route("/api/open_mini", post(handle_open_mini))
        .route("/api/play/scrobble", post(handle_manual_scrobble))
        .route("/api/import_lastfm", post(handle_import_lastfm))
        .route("/api/mode", post(handle_mode))
        .route("/api/love", post(handle_love))
        .route("/api/logout", post(handle_logout))
        .route("/api/quit", post(handle_quit))
        .route("/api/restart", post(handle_restart))
        .layer(CorsLayer::permissive())
        .with_state(app)
}

// ---------- Handlers ----------

async fn handle_index() -> Response {
    let html = get_web_file("index.html");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(html))
        .unwrap()
}

async fn handle_mini() -> Response {
    let html = get_web_file("mini.html");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(html))
        .unwrap()
}

async fn handle_state(State(app): State<Arc<App>>) -> impl IntoResponse {
    Json(app.state().await)
}

async fn handle_recent(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(50);
    Json(app.db.recent(limit))
}

async fn handle_stats(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let days = params
        .get("days")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(7);
    Json(app.stats(days))
}

async fn handle_radar(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let track = params.get("track").map(|s| s.trim()).unwrap_or("");
    let artist = params.get("artist").map(|s| s.trim()).unwrap_or("");
    Json(app.radar(track, artist).await)
}

async fn handle_roam(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let user = params.get("user").map(|s| s.as_str());
    let hide_heard = params.get("hide_heard").map(|s| s.as_str()) != Some("0");
    let shuffle = params.get("shuffle").map(|s| s.as_str()) == Some("1");
    Json(app.roam(user, hide_heard, shuffle).await)
}

async fn handle_ncm_match(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let track = params.get("track").map(|s| s.trim()).unwrap_or("");
    let artist = params.get("artist").map(|s| s.trim()).unwrap_or("");
    let res = app.ncm.match_song(track, artist).await;
    Json(res)
}

async fn handle_ncm_song(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let id = params
        .get("id")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let res = app.ncm.detail(id).await.unwrap_or_default();
    Json(res)
}

async fn handle_recs_stats(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let days = params
        .get("days")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(30);
    Json(app.db.recs_stats(days))
}

async fn handle_get_feedback(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100);
    Json(serde_json::json!({
        "ok": true,
        "list": app.db.get_feedback_list(limit)
    }))
}

async fn handle_export_csv(State(app): State<Arc<App>>) -> Response {
    let csv_bytes = app.export_csv();
    let now_str = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("musichub_history_{}.csv", now_str);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8-sig"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)).unwrap(),
    );

    (headers, csv_bytes).into_response()
}

async fn handle_export_backup(State(app): State<Arc<App>>) -> Response {
    let db_bytes = app.export_backup();
    let now_str = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("musichub_backup_{}.db", now_str);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-sqlite3"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)).unwrap(),
    );

    (headers, db_bytes).into_response()
}

async fn handle_auth_start(State(app): State<Arc<App>>) -> Response {
    if !app.cfg.has_credentials() {
        return Redirect::to("/#settings").into_response();
    }
    let cb = format!("{}/callback", app.base_url());
    let url = app.lf.auth_url(&cb);
    Redirect::to(&url).into_response()
}

async fn handle_callback(
    State(app): State<Arc<App>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let token = match params.get("token") {
        Some(t) if !t.is_empty() => t,
        _ => return Redirect::to("/#settings").into_response(),
    };

    match app.lf.get_session(token).await {
        Ok((sk, name)) => {
            app.cfg.update(|d| {
                d.session_key = sk;
                d.username = name.clone();
            });
            tracing::info!("Authorized Last.fm session for user {}", name);
            Redirect::to("/?auth=ok#settings").into_response()
        }
        Err(e) => {
            tracing::warn!("Last.fm auth callback failed: {e}");
            Redirect::to("/?auth=fail#settings").into_response()
        }
    }
}

// ---------- POST Handlers ----------

#[derive(Deserialize)]
struct PlayReq {
    id: i64,
}

async fn handle_play(Json(body): Json<PlayReq>) -> impl IntoResponse {
    App::play_in_client(body.id);
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct PlayArtistReq {
    artist: String,
}

async fn handle_play_artist(
    State(app): State<Arc<App>>,
    Json(body): Json<PlayArtistReq>,
) -> impl IntoResponse {
    let s = app.play_artist(&body.artist).await;
    Json(serde_json::json!({ "ok": s.is_some(), "song": s }))
}

#[derive(Deserialize)]
struct ConfigReq {
    api_key: Option<String>,
    api_secret: Option<String>,
    username: Option<String>,
    scrobble_unmatched: Option<bool>,
    apps: Option<Vec<String>>,
}

async fn handle_post_config(
    State(app): State<Arc<App>>,
    Json(body): Json<ConfigReq>,
) -> impl IntoResponse {
    app.cfg.update(|d| {
        if let Some(key) = body.api_key {
            let trimmed = key.trim().to_string();
            if trimmed != d.api_key {
                d.api_key = trimmed;
                d.session_key.clear(); // Reset session key on API key change
            }
        }
        if let Some(secret) = body.api_secret {
            let trimmed = secret.trim().to_string();
            if !trimmed.is_empty() {
                d.api_secret = trimmed;
            }
        }
        if let Some(u) = body.username {
            d.username = u.trim().to_string();
        }
        if let Some(su) = body.scrobble_unmatched {
            d.scrobble_unmatched = su;
        }
        if let Some(apps_list) = body.apps {
            let cleaned: Vec<String> = apps_list
                .into_iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
            if !cleaned.is_empty() {
                d.apps = cleaned;
            }
        }
    });

    Json(serde_json::json!({ "ok": true, "config": app.cfg.public() }))
}

#[derive(Deserialize)]
struct DeletePlayReq {
    id: i64,
}

async fn handle_delete_play(
    State(app): State<Arc<App>>,
    Json(body): Json<DeletePlayReq>,
) -> impl IntoResponse {
    app.db.delete_play(body.id);
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ClearSkipReq {
    title: Option<String>,
    artist: Option<String>,
}

async fn handle_clear_skip(
    State(app): State<Arc<App>>,
    Json(body): Json<ClearSkipReq>,
) -> impl IntoResponse {
    app.db
        .clear_skip(body.title.as_deref().unwrap_or(""), body.artist.as_deref().unwrap_or(""));
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct FeedbackReq {
    title: Option<String>,
    artist: Option<String>,
    action: Option<String>,
}

async fn handle_post_feedback(
    State(app): State<Arc<App>>,
    Json(body): Json<FeedbackReq>,
) -> impl IntoResponse {
    let ok = app.db.set_feedback(
        body.title.as_deref().unwrap_or(""),
        body.artist.as_deref().unwrap_or(""),
        body.action.as_deref().unwrap_or(""),
    );
    Json(serde_json::json!({ "ok": ok }))
}

async fn handle_open_mini(State(app): State<Arc<App>>) -> impl IntoResponse {
    let ok = app.open_mini_window().await;
    Json(serde_json::json!({ "ok": ok }))
}

#[derive(Deserialize)]
struct ScrobbleReq {
    id: Option<i64>,
}

async fn handle_manual_scrobble(
    State(app): State<Arc<App>>,
    Json(body): Json<ScrobbleReq>,
) -> Response {
    match app.manual_scrobble(body.id) {
        Ok(res) => Json(res).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": err })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ImportLastfmReq {
    user: Option<String>,
}

async fn handle_import_lastfm(
    State(app): State<Arc<App>>,
    Json(body): Json<ImportLastfmReq>,
) -> Response {
    match app.import_lastfm(body.user.as_deref()).await {
        Ok(res) => Json(res).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": err })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ModeReq {
    mode: String,
}

async fn handle_mode(State(app): State<Arc<App>>, Json(body): Json<ModeReq>) -> Response {
    if body.mode == "scrobble" && !app.cfg.is_authorized() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "请先完成 Last.fm 授权" })),
        )
            .into_response();
    }
    if body.mode != "record" && body.mode != "scrobble" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "bad mode" })),
        )
            .into_response();
    }
    let is_scrobble = body.mode == "scrobble";
    app.cfg.update(|d| d.mode = body.mode);
    if is_scrobble {
        app.scrobbler.kick();
    }
    Json(serde_json::json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct LoveReq {
    artist: Option<String>,
    track: Option<String>,
}

async fn handle_love(State(app): State<Arc<App>>, Json(body): Json<LoveReq>) -> Response {
    if !app.cfg.is_authorized() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "请先完成 Last.fm 授权" })),
        )
            .into_response();
    }
    let now = app.monitor.snapshot();
    let artist = body.artist.or_else(|| now.as_ref().map(|n| n.artist.clone()));
    let track = body.track.or_else(|| now.as_ref().map(|n| n.title.clone()));
    match (artist, track) {
        (Some(a), Some(t)) if !a.is_empty() && !t.is_empty() => match app.lf.love(&a, &t).await {
            Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": e })),
            )
                .into_response(),
        },
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "当前没有在播放" })),
        )
            .into_response(),
    }
}

async fn handle_logout(State(app): State<Arc<App>>) -> impl IntoResponse {
    app.cfg.update(|d| {
        d.session_key.clear();
        d.username.clear();
        d.mode = "record".to_string();
    });
    Json(serde_json::json!({ "ok": true }))
}

async fn handle_quit() -> impl IntoResponse {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::process::exit(0);
    });
    Json(serde_json::json!({ "ok": true }))
}

async fn handle_restart() -> impl IntoResponse {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).spawn();
        }
        std::process::exit(0);
    });
    Json(serde_json::json!({ "ok": true }))
}

pub async fn run_server(app: Arc<App>, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let router = make_router(app);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on http://{}", addr);
    axum::serve(listener, router).await?;
    Ok(())
}
