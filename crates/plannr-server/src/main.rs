mod db;
mod error;
mod mcp;
mod plugins;
mod routes;
mod state;
mod ws;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")))
        .init();

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8795);
    let db_path = PathBuf::from(std::env::var("PLANNR_DB").unwrap_or_else(|_| "data/plannr.db".into()));
    let web_dir = PathBuf::from(std::env::var("PLANNR_WEB").unwrap_or_else(|_| "web/dist".into()));
    let token = std::env::var("PLANNR_TOKEN").ok().filter(|t| !t.is_empty()).unwrap_or_else(|| {
        let t: String = uuid::Uuid::new_v4().simple().to_string().chars().take(24).collect();
        tracing::info!("PLANNR_TOKEN not set — generated a token for this run");
        t
    });

    let public = matches!(std::env::var("PLANNR_PUBLIC").as_deref(), Ok("1") | Ok("true"));
    let db = db::Db::open(&db_path)?;
    let state: state::Shared = Arc::new(state::AppState {
        db,
        web_dir: web_dir.clone(),
        projects: Mutex::new(HashMap::new()),
        registry: plugins::registry(),
        hub: Arc::new(ws::Hub::default()),
        active_project: Mutex::new(None),
        token: token.clone(),
        port,
        public,
    });

    let index = web_dir.join("index.html");
    let static_files = ServeDir::new(&web_dir).not_found_service(ServeFile::new(index));

    let guard = axum::middleware::from_fn_with_state(state.clone(), routes::guard);
    let mut app = Router::new()
        .nest("/api", routes::api().layer(guard.clone()))
        .nest("/ws", routes::ws().layer(guard));
    if public {
        // the front page at /, the modeller at /app; no MCP (it would reach every project)
        app = app
            .route_service("/", ServeFile::new(web_dir.join("home.html")))
            .route_service("/app", ServeFile::new(web_dir.join("index.html")))
            .route_service("/app/", ServeFile::new(web_dir.join("index.html")));
    } else {
        app = app.route("/mcp", axum::routing::post(mcp::handle).get(mcp::method_not_allowed));
    }
    let app = app
        .fallback_service(static_files)
        // HTML must not be heuristically cached: a rebuilt bundle has a new hash and the old page would miss it
        .layer(axum::middleware::from_fn(no_cache))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Plannr listening on http://localhost:{port}  (db: {}, web: {})", db_path.display(), web_dir.display());
    if public { tracing::info!("public instance: front page at /, modeller at /app, projects per browser, no MCP"); }
    tracing::info!("MCP token: {token}");
    for p in state.registry.list() { tracing::info!("plugin {} ({}) — {} ops", p.id, p.name, p.ops.len()); }
    tracing::info!("claude mcp add --transport http plannr http://localhost:{port}/mcp --header \"Authorization: Bearer {token}\"");
    if !web_dir.join("index.html").exists() {
        tracing::warn!("{} has no index.html — build the frontend (cd web && npm run build) or set PLANNR_WEB", web_dir.display());
    }
    axum::serve(listener, app).await?;
    Ok(())
}

/// `Cache-Control: no-cache` for the SPA shell and for plugin bundles (fixed file names), so a
/// rebuilt bundle is picked up on the next load; hashed assets stay cacheable.
async fn no_cache(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let plugin = req.uri().path().starts_with("/plugins/");
    let mut res = next.run(req).await;
    let is_html = res.headers().get(axum::http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|v| v.starts_with("text/html")).unwrap_or(false);
    if is_html || plugin { res.headers_mut().insert(axum::http::header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-cache")); }
    res
}
