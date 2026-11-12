use crate::app::App;
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub fn make_router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(handle_root))
        .route("/api/auth/start", get(handle_auth_start))
        .route("/callback", get(handle_callback))
        .route("/api/open_mini", post(handle_open_mini))
        .route("/api/open_console", post(handle_open_console))
        .with_state(app)
}

fn minimal_html_page(title: &str, message: &str, sub: &str) -> Response {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <title>{title}</title>
  <style>
    body {{
      background: #0d1117;
      color: #e6edf3;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Microsoft YaHei", sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100vh;
      margin: 0;
    }}
    .card {{
      background: #161b22;
      border: 1px solid #30363d;
      border-radius: 16px;
      padding: 32px 40px;
      text-align: center;
      max-width: 440px;
      box-shadow: 0 12px 32px rgba(0,0,0,0.45);
    }}
    h2 {{ margin: 0 0 10px; font-size: 20px; }}
    p {{ margin: 0; color: #8b949e; font-size: 14px; line-height: 1.6; }}
  </style>
</head>
<body>
  <div class="card">
    <h2>{message}</h2>
    <p>{sub}</p>
  </div>
  <script>setTimeout(() => window.close(), 1800);</script>
</body>
</html>"#
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(html))
        .unwrap()
}

async fn handle_root(State(app): State<Arc<App>>) -> Response {
    app.open_console_window(None);
    minimal_html_page(
        "Music Hub · 原生客户端",
        "🎵 已唤起 Music Hub 原生桌面窗口",
        "主控台已升级为 GPUI 硬件加速原生窗口，您可以直接关闭此浏览器标签页。",
    )
}

async fn handle_auth_start(State(app): State<Arc<App>>) -> Response {
    if !app.cfg.has_credentials() {
        app.open_console_with_toast(
            Some("settings"),
            Some("请先填写并保存 Last.fm API Key 与 Secret".to_string()),
        );
        return minimal_html_page(
            "Music Hub · 未配置 Key",
            "⚠️ 请先在客户端设置中填写 API Key",
            "已为您切回 Music Hub 客户端设置页。",
        );
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
        _ => {
            app.open_console_with_toast(
                Some("settings"),
                Some("Last.fm 授权未返回有效 Token".to_string()),
            );
            return minimal_html_page(
                "Music Hub · 授权失败",
                "❌ 未收到 Last.fm 授权 Token",
                "请返回 Music Hub 客户端重试。",
            );
        }
    };

    match app.lf.get_session(token).await {
        Ok((sk, name)) => {
            app.cfg.update(|d| {
                d.session_key = sk;
                d.username = name.clone();
            });
            tracing::info!("Authorized Last.fm session for user {}", name);
            app.open_console_with_toast(
                Some("settings"),
                Some(format!("🎉 Last.fm 授权成功！已绑定账号：{}", name)),
            );
            minimal_html_page(
                "Music Hub · 授权成功",
                &format!("🎉 Last.fm 授权成功 ({})", name),
                "已同步至 Music Hub 桌面客户端，您可以关闭此浏览器标签页了。",
            )
        }
        Err(e) => {
            tracing::warn!("Last.fm auth callback failed: {e}");
            app.open_console_with_toast(
                Some("settings"),
                Some(format!("Last.fm 授权失败: {}", e)),
            );
            minimal_html_page(
                "Music Hub · 授权失败",
                "❌ Last.fm 授权失败",
                &format!("错误信息：{}。请返回客户端检查 Key 与 Secret。", e),
            )
        }
    }
}

async fn handle_open_mini(State(app): State<Arc<App>>) -> impl IntoResponse {
    let ok = app.open_mini_window_sync();
    Json(serde_json::json!({ "ok": ok }))
}

async fn handle_open_console(State(app): State<Arc<App>>) -> impl IntoResponse {
    let ok = app.open_console_window(None);
    Json(serde_json::json!({ "ok": ok }))
}

pub async fn run_server(app: Arc<App>, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let router = make_router(app);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("OAuth & IPC listener on http://{}", addr);
    axum::serve(listener, router).await?;
    Ok(())
}
