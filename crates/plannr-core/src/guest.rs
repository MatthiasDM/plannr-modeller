//! The guest half of installable plugins: a `Plugin` compiled to WebAssembly (`wasm32-wasip1`) and run by
//! the server in a sandbox. `plannr_core::wasm_plugin!(MyPlugin)` in the plugin crate exports the functions
//! the server calls; nothing else is needed. See docs/PLUGINS.md → "Installable plugins".
//!
//! ABI (version 1): the module exports
//! - `plannr_abi() -> u32` — 1;
//! - `plannr_alloc(len: u32) -> u32` / `plannr_free(ptr: u32, len: u32)` — buffers in its memory;
//! - `plannr_call(ptr: u32, len: u32) -> u64` — takes a JSON request `{ "call": …, … }` and returns
//!   `(ptr << 32) | len` of a response buffer: a little-endian u32 with the length of a JSON header
//!   `{ "ok": value }` or `{ "err": "message" }`, the header, then raw bytes (an export's file). The server
//!   frees both buffers.

use std::sync::Mutex;

use serde_json::{json, Value};

use crate::compile::CompiledScene;
use crate::geom::Mat4;
use crate::model::{Custom, Document};
use crate::ops::OpError;
use crate::plugin::{Plugin, Side};

pub const ABI_VERSION: u32 = 1;

/// The document the server sent with `prepare`: `expand` calls without a document use it.
static DOC: Mutex<Option<Document>> = Mutex::new(None);

pub fn alloc(len: u32) -> u32 {
    let mut v: Vec<u8> = Vec::with_capacity(len.max(1) as usize);
    let p = v.as_mut_ptr(); std::mem::forget(v); p as usize as u32
}
/// # Safety
/// `ptr` / `len` must come from `alloc` (or a response of `call`) and be freed once.
pub unsafe fn free(ptr: u32, len: u32) {
    drop(Vec::from_raw_parts(ptr as usize as *mut u8, 0, len.max(1) as usize));
}

fn respond(header: Value, bytes: &[u8]) -> u64 {
    let h = serde_json::to_vec(&header).unwrap_or_else(|_| b"{\"err\":\"unserialisable response\"}".to_vec());
    let mut out = Vec::with_capacity(4 + h.len() + bytes.len());
    out.extend_from_slice(&(h.len() as u32).to_le_bytes()); out.extend_from_slice(&h); out.extend_from_slice(bytes);
    let len = out.len() as u32;
    let ptr = alloc(len);
    // SAFETY: `ptr` has room for `len` bytes (just allocated)
    unsafe { std::ptr::copy_nonoverlapping(out.as_ptr(), ptr as usize as *mut u8, out.len()); }
    ((ptr as u64) << 32) | len as u64
}

fn field<T: serde::de::DeserializeOwned>(req: &mut Value, k: &str) -> Result<T, OpError> {
    serde_json::from_value(req.get_mut(k).map(Value::take).unwrap_or(Value::Null)).map_err(|e| OpError(format!("bad `{k}`: {e}")))
}
fn doc_or_cached(req: &mut Value) -> Result<Document, OpError> {
    if req.get("doc").is_some_and(|d| !d.is_null()) {
        let d: Document = field(req, "doc")?;
        *DOC.lock().unwrap() = Some(d.clone());
        return Ok(d);
    }
    DOC.lock().unwrap().clone().ok_or_else(|| OpError("no document: call prepare first".into()))
}

/// Handle one request.
pub fn call(p: &dyn Plugin, ptr: u32, len: u32) -> u64 {
    // SAFETY: the host wrote `len` bytes at `ptr` (from `alloc`) and frees them after the call
    let bytes = unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) };
    let mut req: Value = match serde_json::from_slice(bytes) { Ok(v) => v, Err(e) => return respond(json!({ "err": format!("bad request: {e}") }), &[]) };
    match handle(p, &mut req) {
        Ok((v, b)) => respond(json!({ "ok": v }), &b),
        Err(e) => respond(json!({ "err": e.0 }), &[]),
    }
}

fn handle(p: &dyn Plugin, req: &mut Value) -> Result<(Value, Vec<u8>), OpError> {
    let call = req.get("call").and_then(|c| c.as_str()).unwrap_or("").to_string();
    let none = Vec::new();
    Ok(match call.as_str() {
        "describe" => (json!({ "info": p.describe(), "aliases": p.aliases(), "exports": p.exports() }), none),
        "prepare" => { let d: Document = field(req, "doc")?; *DOC.lock().unwrap() = Some(d); (Value::Null, none) }
        "expand" => {
            let e: Custom = field(req, "entity")?; let d = doc_or_cached(req)?;
            (serde_json::to_value(p.expand(&e, &d)?).unwrap(), none)
        }
        "apply" => {
            let op: String = field(req, "op")?; let data: Value = field(req, "data")?; let mut d: Document = field(req, "doc")?;
            let r = p.apply(&op, data, &mut d)?;
            (json!({ "result": r, "doc": d }), none)
        }
        "transform" => {
            let mut e: Custom = field(req, "entity")?; let m: Mat4 = field(req, "matrix")?; let d = doc_or_cached(req)?;
            p.transform(&mut e, &m, &d)?; (serde_json::to_value(e).unwrap(), none)
        }
        "paint" => {
            let mut e: Custom = field(req, "entity")?; let key: String = field(req, "faceKey")?; let mat: String = field(req, "material")?; let side: String = field(req, "side")?;
            p.paint(&mut e, &key, &mat, Side::parse(&side))?; (serde_json::to_value(e).unwrap(), none)
        }
        "metrics" => { let d = doc_or_cached(req)?; (p.metrics(&d).unwrap_or(Value::Null), none) }
        "on_load" => {
            let mut d: Document = field(req, "doc")?; let before = d.clone();
            p.on_load(&mut d);
            (if d == before { Value::Null } else { serde_json::to_value(d).unwrap() }, none)
        }
        "export" => {
            let format: String = field(req, "format")?; let scene: CompiledScene = field(req, "scene")?; let d: Document = field(req, "doc")?; let options: Value = field(req, "options")?;
            let f = p.export(&format, &scene, &d, &options)?;
            (json!({ "report": f.report }), f.bytes)
        }
        other => return Err(OpError(format!("unknown call {other}"))),
    })
}

/// Export a plugin from a crate built for `wasm32-wasip1`: `plannr_core::wasm_plugin!(MyPlugin);`
/// (any expression that builds the plugin). Does nothing on other targets, so the crate still builds and
/// tests natively.
#[macro_export]
macro_rules! wasm_plugin {
    ($make:expr) => {
        #[cfg(target_family = "wasm")]
        #[doc(hidden)]
        pub mod __plannr_guest {
            use super::*;
            fn plugin() -> &'static dyn $crate::plugin::Plugin {
                static P: ::std::sync::OnceLock<Box<dyn $crate::plugin::Plugin>> = ::std::sync::OnceLock::new();
                P.get_or_init(|| Box::new($make)).as_ref()
            }
            #[no_mangle] pub extern "C" fn plannr_abi() -> u32 { $crate::guest::ABI_VERSION }
            #[no_mangle] pub extern "C" fn plannr_alloc(len: u32) -> u32 { $crate::guest::alloc(len) }
            #[no_mangle] pub unsafe extern "C" fn plannr_free(ptr: u32, len: u32) { $crate::guest::free(ptr, len) }
            #[no_mangle] pub extern "C" fn plannr_call(ptr: u32, len: u32) -> u64 { $crate::guest::call(plugin(), ptr, len) }
        }
    };
}
