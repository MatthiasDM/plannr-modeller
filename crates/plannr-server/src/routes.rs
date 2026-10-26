//! REST API under /api and the WebSocket upgrade.

use axum::body::Body;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, Request, State};
use axum::middleware::Next;
use axum::Extension;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use plannr_core::{compile, export_glb, sample_document, CompileOptions, Document, Entity, Op};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::ProjectSummary;
use crate::error::{ApiError, ApiResult};
use crate::state::{Owner, Shared};

pub fn api() -> Router<Shared> {
    Router::new()
        .route("/health", get(health))
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/{id}", get(get_project).patch(rename_project).delete(delete_project))
        .route("/projects/{id}/document", axum::routing::put(put_document))
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
        .route("/compile", post(compile_document))
        .route("/plugins", get(plugins))
        .route("/sample", post(sample))
}

pub fn ws() -> Router<Shared> {
    Router::new().route("/projects/{id}", get(ws_upgrade))
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "version": plannr_core::VERSION, "name": "plannr" }))
}

/// Public instances: every request names its browser (`X-Plannr-Owner` header or `?owner=`), and a
/// project can only be reached by the browser that made it. Private instances pass everything.
pub async fn guard(State(s): State<Shared>, mut req: Request, next: Next) -> Response {
    if !s.public { req.extensions_mut().insert(Owner(None)); return next.run(req).await; }
    let path = req.uri().path().to_string();
    let from_header = req.headers().get("x-plannr-owner").and_then(|v| v.to_str().ok()).map(|v| v.to_string());
    let from_query = req.uri().query().and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("owner=").map(|v| v.to_string())));
    let owner = from_header.or(from_query).filter(|o| (16..=64).contains(&o.len()) && o.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let open = path.ends_with("/health") || path.ends_with("/plugins");
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
pub struct OpsBody { pub ops: Vec<Op>, #[serde(default)] pub label: Option<String> }

async fn apply_ops(State(s): State<Shared>, Path(id): Path<String>, Json(b): Json<OpsBody>) -> ApiResult<Json<Value>> {
    if b.ops.is_empty() { return Err(ApiError::BadRequest("ops is empty".into())); }
    let label = b.label.clone().unwrap_or_else(|| default_label(&b.ops));
    let (version, results) = s.apply_ops(&id, &b.ops, &label, "api")?;
    Ok(Json(json!({ "version": version, "results": results })))
}

pub fn default_label(ops: &[Op]) -> String {
    let n = ops[0].name().replace('_', " ");
    let mut c = n.chars();
    let cap = match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => n };
    if ops.len() > 1 { format!("{cap} (+{})", ops.len() - 1) } else { cap }
}

async fn preview(State(s): State<Shared>, Path(id): Path<String>, Json(b): Json<OpsBody>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(s.preview(&id, &b.ops)?)?))
}

async fn undo(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let (version, label) = s.undo(&id, "api")?;
    Ok(Json(json!({ "version": version, "label": label })))
}
async fn redo(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let (version, label) = s.redo(&id, "api")?;
    Ok(Json(json!({ "version": version, "label": label })))
}
async fn history(State(s): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(s.history(&id)?)?))
}

#[derive(Deserialize)]
struct CompileQuery { #[serde(default)] section: Option<String> }

async fn compile_project(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<CompileQuery>) -> ApiResult<Json<Value>> {
    let opts = CompileOptions { ignore_sections: matches!(q.section.as_deref(), Some("0") | Some("false")) };
    Ok(Json(serde_json::to_value(s.compile(&id, &opts)?)?))
}

#[derive(Deserialize)]
struct CompileBody { document: Document }
async fn compile_document(State(s): State<Shared>, Json(b): Json<CompileBody>) -> ApiResult<Json<Value>> {
    let mut doc = b.document;
    doc.normalize();
    s.registry.on_load(&mut doc);
    Ok(Json(serde_json::to_value(compile(&doc, &s.registry, &CompileOptions::default()))?))
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
    let scene = s.compile(&id, &CompileOptions { ignore_sections: true })?;
    let bytes = export_glb(&scene);
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "model/gltf-binary")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}.glb\"", safe_name(&p.name)))
        .body(Body::from(bytes))
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

async fn plugins(State(s): State<Shared>) -> Json<Value> {
    let mut list: Vec<Value> = s.registry.list().into_iter().map(|p| {
        let rel = format!("plugins/{}.js", p.id);
        let script = if s.web_dir.join(&rel).exists() { json!(format!("/{rel}")) } else { Value::Null };
        let mut v = serde_json::to_value(&p).unwrap_or(Value::Null);
        v["script"] = script;
        v
    }).collect();
    // browser-only plugins: a bundle in <web>/plugins/ without an engine counterpart
    if let Ok(dir) = std::fs::read_dir(s.web_dir.join("plugins")) {
        let mut extra: Vec<String> = dir.filter_map(|e| e.ok()).filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".js").map(|id| id.to_string())
        }).filter(|id| !list.iter().any(|v| v["id"] == json!(id))).collect();
        extra.sort();
        for id in extra {
            list.push(json!({ "id": id, "name": id, "entityTypes": [], "ops": [], "script": format!("/plugins/{id}.js"), "browserOnly": true }));
        }
    }
    Json(json!(list))
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
