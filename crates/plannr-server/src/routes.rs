//! REST API under /api and the WebSocket upgrade.

use axum::body::Body;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::HeaderMap;
use std::net::SocketAddr;
use axum::middleware::Next;
use axum::Extension;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use plannr_core::{compile, export_glb_textured, sample_document, CompileOptions, Document, Entity, Op};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::ProjectSummary;
use crate::error::{ApiError, ApiResult};
use crate::state::{Owner, Shared};

/// A whole document in one request (a new project from a file, Units, Axes, Scenes): it carries its texture images.
const DOCUMENT_LIMIT: usize = 24 << 20;

pub fn api() -> Router<Shared> {
    Router::new()
        .route("/health", get(health))
        .route("/mcp-token", get(mcp_token))
        .route("/projects", get(list_projects).post(create_project).layer(axum::extract::DefaultBodyLimit::max(DOCUMENT_LIMIT)))
        .route("/projects/{id}", get(get_project).patch(rename_project).delete(delete_project))
        .route("/projects/{id}/document", axum::routing::put(put_document).layer(axum::extract::DefaultBodyLimit::max(DOCUMENT_LIMIT)))
        .route("/projects/{id}/duplicate", post(duplicate))
        .route("/projects/{id}/ops", post(apply_ops))
        .route("/projects/{id}/preview", post(preview))
        .route("/projects/{id}/undo", post(undo))
        .route("/projects/{id}/redo", post(redo))
        .route("/projects/{id}/history", get(history))
        .route("/projects/{id}/compile", get(compile_project))
        .route("/projects/{id}/entities", get(entities))
        .route("/projects/{id}/export.glb", get(export_glb_route))
        .route("/projects/{id}/export.json", get(export_json))
        .route("/projects/{id}/export/{format}", get(export_plugin_route))
        .route("/compile", post(compile_document))
        .route("/plugins", get(plugins))
        .route("/plugins/install", post(install_plugin).layer(axum::extract::DefaultBodyLimit::max(96 << 20)))
        .route("/plugins/{id}/enabled", post(enable_plugin))
        .route("/plugins/{id}", axum::routing::delete(uninstall_plugin))
        .route("/sample", post(sample))
}

pub fn ws() -> Router<Shared> {
    Router::new().route("/projects/{id}", get(ws_upgrade))
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "version": plannr_core::VERSION, "name": "plannr" }))
}

/// The MCP token, for the "Connect an AI" panel in the app. Only on a private instance, only to a
/// browser on the machine that runs the server (loopback peer), and only to Plannr's own page: a
/// cross-origin page would send an `Origin` for another host (and `Sec-Fetch-Site: cross-site`), and
/// gets nothing even though CORS is permissive for the rest of the API.
async fn mcp_token(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Response {
    let err = |code: StatusCode, msg: &str| (code, Json(json!({ "error": msg }))).into_response();
    if s.public { return err(StatusCode::NOT_FOUND, "no MCP on a public instance"); }
    if !peer.ip().is_loopback() { return err(StatusCode::FORBIDDEN, "the token is only shown to a browser on the machine that runs plannr-server"); }
    let hdr = |h: header::HeaderName| headers.get(h).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let host = hdr(header::HOST);
    let origin = hdr(header::ORIGIN);
    if !origin.is_empty() {
        let same = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")).map(|o| o == host).unwrap_or(false);
        if !same { return err(StatusCode::FORBIDDEN, "same-origin only"); }
    }
    let site = hdr(header::HeaderName::from_static("sec-fetch-site"));
    if !site.is_empty() && site != "same-origin" && site != "none" { return err(StatusCode::FORBIDDEN, "same-origin only"); }
    Json(json!({ "token": s.token, "endpoint": format!("http://localhost:{}/mcp", s.port) })).into_response()
}

/// Public instances: every request names its browser (`X-Plannr-Owner` header or `?owner=`), and a
/// project can only be reached by the browser that made it. Private instances pass everything.
pub async fn guard(State(s): State<Shared>, mut req: Request, next: Next) -> Response {
    if !s.public { req.extensions_mut().insert(Owner(None)); return next.run(req).await; }
    let path = req.uri().path().to_string();
    let from_header = req.headers().get("x-plannr-owner").and_then(|v| v.to_str().ok()).map(|v| v.to_string());
    let from_query = req.uri().query().and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("owner=").map(|v| v.to_string())));
    let owner = from_header.or(from_query).filter(|o| (16..=64).contains(&o.len()) && o.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let open = path.ends_with("/health") || path.ends_with("/plugins") || path.ends_with("/mcp-token");
    let Some(owner) = owner else {
        if open { req.extensions_mut().insert(Owner(None)); return next.run(req).await; }
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "this public instance needs an X-Plannr-Owner header" }))).into_response();
    };
    let segs: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).collect();
    if let Some(i) = segs.iter().position(|x| *x == "projects") {
        if let Some(id) = segs.get(i + 1) {
            match s.db.owner_of(id) {
                Ok(Some(o)) if o == owner => {}
                _ => return (StatusCode::NOT_FOUND, Json(json!({ "error": "project not found" }))).into_response(),
            }
        }
    }
    req.extensions_mut().insert(Owner(Some(owner)));
    next.run(req).await
}

async fn list_projects(State(s): State<Shared>, Extension(owner): Extension<Owner>) -> ApiResult<Json<Vec<ProjectSummary>>> { Ok(Json(s.db.list_for(owner.0.as_deref())?)) }

#[derive(Deserialize)]
struct CreateBody { name: String, #[serde(default)] document: Option<Document> }

async fn create_project(State(s): State<Shared>, Extension(owner): Extension<Owner>, Json(b): Json<CreateBody>) -> ApiResult<(StatusCode, Json<ProjectSummary>)> {
    let name = b.name.trim();
    if name.is_empty() { return Err(ApiError::BadRequest("name is required".into())); }
    Ok((StatusCode::CREATED, Json(s.create_for(name, b.document, &owner)?)))
}

async fn get_project(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let p = s.db.get(&id)?;
    let doc = s.document(&id)?;
    Ok(Json(json!({ "id": p.id, "name": p.name, "createdAt": p.created_at, "updatedAt": p.updated_at, "document": doc })))
}

#[derive(Deserialize)]
struct RenameBody { name: String }
async fn rename_project(State(s): State<Shared>, Path(id): Path<String>, Json(b): Json<RenameBody>) -> ApiResult<Json<ProjectSummary>> {
    let name = b.name.trim();
    if name.is_empty() { return Err(ApiError::BadRequest("name is required".into())); }
    Ok(Json(s.rename(&id, name)?))
}

async fn delete_project(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    s.db.delete(&id)?;
    s.evict(&id);
    Ok(StatusCode::NO_CONTENT)
}

async fn put_document(State(s): State<Shared>, Path(id): Path<String>, Json(doc): Json<Document>) -> ApiResult<Json<Value>> {
    let v = s.replace_document(&id, doc, "api")?;
    Ok(Json(json!({ "version": v })))
}

async fn duplicate(State(s): State<Shared>, Extension(owner): Extension<Owner>, Path(id): Path<String>) -> ApiResult<(StatusCode, Json<ProjectSummary>)> {
    let p = s.db.get(&id)?;
    let doc = s.document(&id)?;
    Ok((StatusCode::CREATED, Json(s.create_for(&format!("{} copy", p.name), Some(doc), &owner)?)))
}

#[derive(Deserialize)]
pub struct OpsBody { pub ops: Vec<Op>, #[serde(default)] pub label: Option<String>, #[serde(default)] pub partial: bool }

async fn apply_ops(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<CompileQuery>, Json(b): Json<OpsBody>) -> ApiResult<Response> {
    if b.ops.is_empty() { return Err(ApiError::BadRequest("ops is empty".into())); }
    let label = b.label.clone().unwrap_or_else(|| default_label(&b.ops));
    if !b.partial {
        let (version, results) = s.apply_ops(&id, &b.ops, &label, "api")?;
        return Ok(Json(json!({ "version": version, "results": results })).into_response());
    }
    let (version, results, changes) = s.apply_ops_partial(&id, &b.ops, &label, "api", &q.options())?;
    Ok(Json(ChangedReply { version, results: Some(&results), label: None, changes: changes.as_ref().map(ChangesReply::from) }).into_response())
}

/// A commit, undo or redo with `partial`: what changed, compiled on its own (`scene` + `dirty`), or neither
/// when the browser has to fetch the whole scene.
#[derive(Serialize)]
struct ChangedReply<'a> {
    version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    results: Option<&'a [plannr_core::OpResult]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(flatten)]
    changes: Option<ChangesReply<'a>>,
}
#[derive(Serialize)]
struct ChangesReply<'a> { scene: &'a plannr_core::CompiledScene, dirty: &'a [String], reconcile: bool }
impl<'a> From<&'a crate::state::Changes> for ChangesReply<'a> {
    fn from(c: &'a crate::state::Changes) -> Self { ChangesReply { scene: &c.scene, dirty: &c.dirty, reconcile: c.reconcile } }
}

pub fn default_label(ops: &[Op]) -> String {
    let n = ops[0].name().replace('_', " ");
    let mut c = n.chars();
    let cap = match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => n };
    if ops.len() > 1 { format!("{cap} (+{})", ops.len() - 1) } else { cap }
}

/// A compiled scene, written straight to JSON: going through `serde_json::Value` would print every f32 with
/// f64 digits (3.549999952316284 for 3.55), more than doubling the biggest replies.
#[derive(Serialize)]
struct PartialScene { #[serde(flatten)] scene: plannr_core::CompiledScene, partial: bool, dirty: Vec<String> }

async fn preview(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<CompileQuery>, Json(b): Json<OpsBody>) -> ApiResult<Response> {
    let opts = q.options();
    if !b.partial { return Ok(Json(s.preview(&id, &b.ops, &opts)?).into_response()); }
    Ok(match s.preview_partial(&id, &b.ops, &opts)? {
        (scene, Some(dirty)) => Json(PartialScene { scene, partial: true, dirty }).into_response(),
        (scene, None) => Json(scene).into_response(),
    })
}

#[derive(Deserialize)]
struct StepQuery { #[serde(default)] partial: Option<String>, #[serde(flatten)] compile: CompileQuery }
async fn undo(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<StepQuery>) -> ApiResult<Response> { step(s, id, q, true) }
async fn redo(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<StepQuery>) -> ApiResult<Response> { step(s, id, q, false) }
fn step(s: Shared, id: String, q: StepQuery, back: bool) -> ApiResult<Response> {
    let opts = flag(&q.partial).then(|| q.compile.options());
    let (version, label, changes) = s.step(&id, "api", back, opts.as_ref())?;
    Ok(Json(ChangedReply { version, results: None, label: Some(label), changes: changes.as_ref().map(ChangesReply::from) }).into_response())
}
async fn history(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(s.history(&id)?)?))
}

#[derive(Deserialize)]
struct CompileQuery { #[serde(default)] section: Option<String>, #[serde(default)] hidden: Option<String> }
impl CompileQuery {
    /// `section=0`: uncut geometry (the viewer cuts with `cuts`); `hidden=1`: hidden entities as ghosts.
    fn options(&self) -> CompileOptions { CompileOptions { ignore_sections: matches!(self.section.as_deref(), Some("0") | Some("false")), show_hidden: flag(&self.hidden), only: None } }
}
fn flag(v: &Option<String>) -> bool { matches!(v.as_deref(), Some("1") | Some("true")) }

async fn compile_project(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<CompileQuery>) -> ApiResult<Json<plannr_core::CompiledScene>> {
    Ok(Json(s.compile(&id, &q.options())?))
}

#[derive(Deserialize)]
struct CompileBody { document: Document }
async fn compile_document(State(s): State<Shared>, Json(b): Json<CompileBody>) -> ApiResult<Json<plannr_core::CompiledScene>> {
    let mut doc = b.document;
    doc.normalize();
    s.registry.on_load(&mut doc);
    Ok(Json(compile(&doc, &s.registry, &CompileOptions::default())))
}

#[derive(Deserialize)]
struct EntityQuery { #[serde(default)] kind: Option<String>, #[serde(default)] layer: Option<String> }

pub fn flatten_entities(doc: &Document, kind: Option<&str>, layer: Option<&str>) -> Vec<Value> {
    let layer_id = layer.and_then(|l| doc.layer_by_ref(l)).map(|l| l.id.clone());
    let mut out = vec![];
    doc.visit(|v| {
        if let Some(k) = kind { if v.entity.kind() != k { return; } }
        if let Some(l) = &layer_id { if v.entity.layer() != l { return; } }
        let mut e = serde_json::to_value(v.entity).unwrap_or(Value::Null);
        if let Entity::Group(g) = v.entity {
            // avoid dumping the whole subtree for every nested level
            e["entities"] = json!(g.entities.iter().map(|c| c.id()).collect::<Vec<_>>());
        }
        e["path"] = json!(v.path);
        out.push(e);
    });
    out
}

async fn entities(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<EntityQuery>) -> ApiResult<Json<Value>> {
    let doc = s.document(&id)?;
    Ok(Json(json!(flatten_entities(&doc, q.kind.as_deref(), q.layer.as_deref()))))
}

fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

async fn export_glb_route(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Response> {
    let p = s.db.get(&id)?;
    let scene = s.compile(&id, &CompileOptions { ignore_sections: true, ..Default::default() })?;
    let bytes = export_glb_textured(&scene, &s.document(&id)?.materials);
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "model/gltf-binary")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}.glb\"", safe_name(&p.name)))
        .body(Body::from(bytes))
        .unwrap())
}

/// Run a plugin export (`stl`, `3mf`…). `options.ids` (entity ids, nested ones count as their top-level
/// entity) limits the export to those entities; without it everything visible goes in.
/// Returns the file, the format and a file name.
pub fn run_export(s: &Shared, id: &str, format: &str, mut options: Value) -> ApiResult<(plannr_core::ExportFile, plannr_core::ExportFormat, String)> {
    let p = s.db.get(id)?;
    let plugin = s.registry.exporter(format).ok_or_else(|| ApiError::BadRequest(format!("no plugin writes .{format} (available: {})", s.registry.exports().iter().map(|(_, f)| f.format.clone()).collect::<Vec<_>>().join(", "))))?;
    let fmt = plugin.exports().into_iter().find(|f| f.format == format).unwrap();
    let doc = s.document(id)?;
    let wanted: Vec<String> = options.get("ids").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|x| x.to_string())).collect()).unwrap_or_default();
    let mut only = None;
    if !wanted.is_empty() {
        let mut top = std::collections::HashSet::new();
        let mut found = std::collections::HashSet::new();
        doc.visit(|v| {
            let eid = v.entity.id();
            for w in &wanted {
                let base = w.split('#').next().unwrap_or(w);
                if eid == base { top.insert(v.path.first().cloned().unwrap_or_else(|| eid.to_string())); found.insert(w.clone()); }
            }
        });
        if top.is_empty() { return Err(ApiError::BadRequest("none of the ids is in the document".into())); }
        options["ids"] = json!(top.iter().cloned().collect::<Vec<_>>());
        only = Some(top);
    }
    let scene = s.compile(id, &CompileOptions { ignore_sections: true, only, ..Default::default() })?;
    let file = plugin.export(format, &scene, &doc, &options).map_err(|e| ApiError::BadRequest(e.0))?;
    let name = format!("{}.{}", safe_name(&p.name), fmt.format);
    Ok((file, fmt, name))
}

#[derive(Deserialize)]
struct ExportQuery { #[serde(default)] report: Option<String>, #[serde(flatten)] rest: std::collections::HashMap<String, String> }

/// `GET /api/projects/:id/export/:format?ids=a,b&scale=0.01&…` → the file; `report=1` → only the JSON report.
/// Query values that parse as numbers or booleans are passed as such; `ids` is a comma-separated list.
async fn export_plugin_route(State(s): State<Shared>, Path((id, format)): Path<(String, String)>, Query(q): Query<ExportQuery>) -> ApiResult<Response> {
    let mut opts = serde_json::Map::new();
    for (k, v) in q.rest {
        if k == "owner" { continue; }
        let val = if k == "ids" { json!(v.split(',').filter(|x| !x.is_empty()).collect::<Vec<_>>()) }
            else if let Ok(n) = v.parse::<f64>() { json!(n) }
            else if v == "true" || v == "false" { json!(v == "true") }
            else { json!(v) };
        opts.insert(k, val);
    }
    let (file, fmt, name) = run_export(&s, &id, &format, Value::Object(opts))?;
    if q.report.as_deref().is_some_and(|r| r != "0" && r != "false") {
        return Ok(Json(file.report).into_response());
    }
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, fmt.mime)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))
        .header("X-Plannr-Report", serde_json::to_string(&file.report).unwrap_or_default().chars().filter(|c| c.is_ascii() && !c.is_control()).take(4000).collect::<String>())
        .body(Body::from(file.bytes))
        .unwrap())
}

async fn export_json(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Response> {
    let p = s.db.get(&id)?;
    let doc = s.document(&id)?;
    let bytes = serde_json::to_vec_pretty(&doc)?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}.json\"", safe_name(&p.name)))
        .body(Body::from(bytes))
        .unwrap())
}

/// Every plugin: built in (engine and / or browser bundle) or installed from a package, with whether it is
/// enabled. The app loads the `script` of each enabled one (`script` is null for a disabled plugin).
async fn plugins(State(s): State<Shared>) -> Json<Value> {
    let installed = s.packages.installed.lock().unwrap().clone();
    let script_of = |id: &str, enabled: bool| -> Value {
        if !enabled { return Value::Null; }
        let has = s.packages.script(id).is_some() || s.web_dir.join(format!("plugins/{id}.js")).exists();
        if has { json!(format!("/plugins/{id}.js")) } else { Value::Null }
    };
    let mut list: Vec<Value> = s.registry.statuses().into_iter().map(|st| {
        let id = st.info.id.clone();
        let mut v = serde_json::to_value(&st.info).unwrap_or(Value::Null);
        v["enabled"] = json!(st.enabled);
        v["source"] = json!(st.source);
        v["engine"] = json!(true);
        v["script"] = script_of(&id, st.enabled);
        if let Some(m) = installed.get(&id) { merge_manifest(&mut v, m); }
        if st.enabled {
            let exports: Vec<_> = s.registry.exports().into_iter().filter(|(pid, _)| *pid == id).map(|(_, f)| f).collect();
            if !exports.is_empty() { v["exports"] = json!(exports); }
        }
        v
    }).collect();
    let has = |list: &Vec<Value>, id: &str| list.iter().any(|v| v["id"] == json!(id));
    // installed packages without an engine (browser only)
    for (id, m) in &installed {
        if has(&list, id) { continue; }
        let enabled = s.packages.is_enabled(id);
        let mut v = json!({ "id": id, "entityTypes": [], "ops": [], "browserOnly": true, "engine": false, "source": "installed", "enabled": enabled, "script": script_of(id, enabled) });
        merge_manifest(&mut v, m);
        list.push(v);
    }
    // browser-only plugins that ship with the app: a bundle in <web>/plugins/ without an engine counterpart
    if let Ok(dir) = std::fs::read_dir(s.web_dir.join("plugins")) {
        let mut extra: Vec<String> = dir.filter_map(|e| e.ok()).filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".js").map(|id| id.to_string())
        }).filter(|id| !has(&list, id)).collect();
        extra.sort();
        for id in extra {
            let enabled = s.packages.is_enabled(&id);
            list.push(json!({ "id": id, "name": id, "entityTypes": [], "ops": [], "script": script_of(&id, enabled), "browserOnly": true, "engine": false, "source": "builtIn", "enabled": enabled }));
        }
    }
    Json(json!(list))
}
fn merge_manifest(v: &mut Value, m: &crate::packages::Manifest) {
    v["name"] = json!(m.name); v["version"] = json!(m.version); v["description"] = json!(m.description);
    if let Some(h) = &m.homepage { v["homepage"] = json!(h); }
    if let Some(a) = &m.author { v["author"] = json!(a); }
    if !m.suggests.is_empty() { v["suggests"] = json!(m.suggests); }
    if !m.requires.is_empty() { v["requires"] = json!(m.requires); }
}

/// Installing, enabling and removing plugins changes the server for everyone: open on a private instance,
/// on a public one only with the server's token (`Authorization: Bearer …`).
fn may_manage(s: &Shared, headers: &HeaderMap) -> Result<(), Response> {
    if !s.public { return Ok(()); }
    let bearer = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if !bearer.is_empty() && bearer == s.token { return Ok(()); }
    Err((StatusCode::FORBIDDEN, Json(json!({ "error": "managing plugins on this public instance needs its admin token", "needsToken": true }))).into_response())
}

/// `POST /api/plugins/install` with a `.plannr-plugin` file as the body.
async fn install_plugin(State(s): State<Shared>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(r) = may_manage(&s, &headers) { return r; }
    let s2 = s.clone();
    let res = tokio::task::spawn_blocking(move || s2.packages.install(&body, &s2.registry, &s2.builtin)).await;
    match res {
        Ok(Ok(m)) => { tracing::info!("installed plugin {} {}", m.id, m.version); Json(json!({ "installed": m })).into_response() }
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("{e:#}") }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

#[derive(Deserialize)]
struct EnableBody { enabled: bool }

/// `POST /api/plugins/:id/enabled { enabled }` — any plugin, built in or installed.
async fn enable_plugin(State(s): State<Shared>, headers: HeaderMap, Path(id): Path<String>, Json(b): Json<EnableBody>) -> Response {
    if let Err(r) = may_manage(&s, &headers) { return r; }
    let known = s.registry.statuses().iter().any(|p| p.info.id == id) || s.packages.installed.lock().unwrap().contains_key(&id) || s.web_dir.join(format!("plugins/{id}.js")).exists();
    if !known { return (StatusCode::NOT_FOUND, Json(json!({ "error": format!("no plugin {id}") }))).into_response(); }
    s.packages.set_enabled(&id, b.enabled, &s.registry);
    tracing::info!("plugin {id} {}", if b.enabled { "enabled" } else { "disabled" });
    Json(json!({ "id": id, "enabled": b.enabled })).into_response()
}

/// `DELETE /api/plugins/:id` — an installed package (built-in plugins can only be disabled).
async fn uninstall_plugin(State(s): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Err(r) = may_manage(&s, &headers) { return r; }
    match s.packages.uninstall(&id, &s.registry) {
        Ok(()) => { tracing::info!("removed plugin {id}"); Json(json!({ "removed": id })).into_response() }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

/// `GET /plugins/<id>.js`: an installed package's browser bundle, else the one that ships with the app.
pub async fn plugin_script(State(s): State<Shared>, Path(file): Path<String>) -> Response {
    let Some(id) = file.strip_suffix(".js").filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = s.packages.script(id).unwrap_or_else(|| s.web_dir.join("plugins").join(&file));
    match std::fs::read(&path) {
        Ok(b) => Response::builder().header(header::CONTENT_TYPE, "text/javascript; charset=utf-8").header(header::CACHE_CONTROL, "no-cache").body(Body::from(b)).unwrap(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn sample(State(s): State<Shared>, Extension(owner): Extension<Owner>) -> ApiResult<(StatusCode, Json<ProjectSummary>)> {
    Ok((StatusCode::CREATED, Json(s.create_for("Sample", Some(sample_document()), &owner)?)))
}

async fn ws_upgrade(State(s): State<Shared>, Path(id): Path<String>, ws: WebSocketUpgrade) -> Response {
    if s.db.get(&id).is_err() {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "project not found" }))).into_response();
    }
    let hub = s.hub.clone();
    ws.on_upgrade(move |socket| hub.run(socket, id))
}
