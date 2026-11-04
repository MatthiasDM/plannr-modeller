//! Installed plugins' engines: WebAssembly modules (`wasm32-wasip1`, built with `plannr_core::wasm_plugin!`)
//! run in a wasmtime sandbox. They see no files, network or environment (WASI gives them only a clock,
//! random numbers and stderr for panic messages), get a memory cap and a fuel budget per call, and are
//! re-instantiated after a trap. Calls carry JSON (see plannr_core::guest for the ABI).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use plannr_core::compile::CompiledScene;
use plannr_core::geom::Mat4;
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use plannr_core::plugin::{ExportFile, ExportFormat, Plugin, PluginInfo, Side};
use serde::Serialize;
use serde_json::Value;
use wasmtime::{Config, Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;

/// Fuel (≈ instructions) per call, and for exports, which mesh whole models.
const FUEL: u64 = 60_000_000_000;
const FUEL_EXPORT: u64 = 600_000_000_000;
const MEMORY: usize = 2 << 30;

fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| { let mut c = Config::new(); c.consume_fuel(true); Engine::new(&c).expect("wasmtime engine") })
}

struct Ctx { wasi: WasiP1Ctx, limits: StoreLimits }
struct Inner { store: Store<Ctx>, memory: Memory, alloc: TypedFunc<u32, u32>, free: TypedFunc<(u32, u32), ()>, call: TypedFunc<(u32, u32), u64> }

fn instantiate(module: &Module) -> anyhow::Result<Inner> {
    let mut linker: Linker<Ctx> = Linker::new(engine());
    p1::add_to_linker_sync(&mut linker, |c: &mut Ctx| &mut c.wasi)?;
    let wasi = WasiCtxBuilder::new().inherit_stderr().build_p1();
    let mut store = Store::new(engine(), Ctx { wasi, limits: StoreLimitsBuilder::new().memory_size(MEMORY).build() });
    store.limiter(|c| &mut c.limits);
    store.set_fuel(FUEL)?;
    let instance: Instance = linker.instantiate(&mut store, module)?;
    if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") { init.call(&mut store, ())?; }
    let abi = instance.get_typed_func::<(), u32>(&mut store, "plannr_abi").map_err(|_| anyhow::anyhow!("not a Plannr plugin: no plannr_abi export (build it with plannr_core::wasm_plugin!)"))?.call(&mut store, ())?;
    if abi != plannr_core::guest::ABI_VERSION { anyhow::bail!("plugin ABI {abi}, this server speaks {}", plannr_core::guest::ABI_VERSION); }
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| anyhow::anyhow!("no memory export"))?;
    Ok(Inner {
        alloc: instance.get_typed_func(&mut store, "plannr_alloc")?,
        free: instance.get_typed_func(&mut store, "plannr_free")?,
        call: instance.get_typed_func(&mut store, "plannr_call")?,
        memory, store,
    })
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Req<'a> {
    call: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")] doc: Option<&'a Document>,
    #[serde(skip_serializing_if = "Option::is_none")] entity: Option<&'a Custom>,
    #[serde(skip_serializing_if = "Option::is_none")] op: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] data: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")] matrix: Option<&'a Mat4>,
    #[serde(skip_serializing_if = "Option::is_none")] face_key: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] material: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] side: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] scene: Option<&'a CompiledScene>,
    #[serde(skip_serializing_if = "Option::is_none")] options: Option<&'a Value>,
}

pub struct WasmPlugin {
    id: String,
    info: PluginInfo,
    aliases: Vec<&'static str>,
    exports: Vec<ExportFormat>,
    /// Type prefixes this plugin owns (its id, its entity types' prefixes, its aliases).
    prefixes: HashSet<String>,
    module: Module,
    inner: Mutex<Option<Inner>>,
    /// The document `prepare` sent (address, version, entity count), so `expand` can leave it out.
    prepared: Mutex<Option<(usize, u64, usize)>>,
}

impl WasmPlugin {
    pub fn load(bytes: &[u8]) -> anyhow::Result<WasmPlugin> {
        let module = Module::new(engine(), bytes)?;
        let inner = instantiate(&module)?;
        let mut p = WasmPlugin { id: String::new(), info: PluginInfo { id: String::new(), name: String::new(), entity_types: vec![], ops: vec![] }, aliases: vec![], exports: vec![], prefixes: HashSet::new(), module, inner: Mutex::new(Some(inner)), prepared: Mutex::new(None) };
        let (d, _) = p.call(&Req { call: "describe", ..Default::default() }, FUEL).map_err(|e| anyhow::anyhow!(e.0))?;
        p.info = serde_json::from_value(d["info"].clone())?;
        p.id = p.info.id.clone();
        let aliases: Vec<String> = serde_json::from_value(d["aliases"].clone()).unwrap_or_default();
        p.aliases = aliases.into_iter().map(|a| &*Box::leak(a.into_boxed_str())).collect();
        p.exports = serde_json::from_value(d["exports"].clone()).unwrap_or_default();
        p.prefixes = std::iter::once(p.id.clone()).chain(p.aliases.iter().map(|a| a.to_string())).chain(p.info.entity_types.iter().map(|t| t.split('.').next().unwrap_or("").to_string())).collect();
        Ok(p)
    }

    fn call(&self, req: &Req, fuel: u64) -> Result<(Value, Vec<u8>), OpError> {
        let bytes = serde_json::to_vec(req).map_err(|e| OpError(e.to_string()))?;
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() { *g = Some(instantiate(&self.module).map_err(|e| OpError(format!("plugin {}: {e}", self.id)))?); }
        let res = (|| -> anyhow::Result<Vec<u8>> {
            let i = g.as_mut().unwrap();
            i.store.set_fuel(fuel)?;
            let len = bytes.len() as u32;
            let ptr = i.alloc.call(&mut i.store, len)?;
            i.memory.write(&mut i.store, ptr as usize, &bytes)?;
            let packed = i.call.call(&mut i.store, (ptr, len))?;
            i.free.call(&mut i.store, (ptr, len))?;
            let (rp, rl) = ((packed >> 32) as u32, packed as u32);
            let mut out = vec![0u8; rl as usize];
            i.memory.read(&i.store, rp as usize, &mut out)?;
            i.free.call(&mut i.store, (rp, rl))?;
            Ok(out)
        })();
        let out = match res {
            Ok(o) => o,
            Err(e) => {
                // a trap (panic, out of fuel or memory) leaves the instance unusable: start afresh next time
                *g = None; *self.prepared.lock().unwrap() = None;
                let msg = if e.to_string().contains("fuel") { "took too long and was stopped".to_string() } else { format!("{e:#}") };
                return Err(OpError(format!("plugin {}: {msg}", self.id)));
            }
        };
        drop(g);
        if out.len() < 4 { return Err(OpError(format!("plugin {}: empty response", self.id))); }
        let hl = u32::from_le_bytes(out[0..4].try_into().unwrap()) as usize;
        let header: Value = serde_json::from_slice(&out[4..4 + hl.min(out.len() - 4)]).map_err(|e| OpError(format!("plugin {}: bad response: {e}", self.id)))?;
        if let Some(e) = header.get("err").and_then(|e| e.as_str()) { return Err(OpError(e.to_string())); }
        Ok((header.get("ok").cloned().unwrap_or(Value::Null), out[4 + hl..].to_vec()))
    }

    /// Whether the document has anything of this plugin (so loading or compiling it needs the plugin).
    fn owns(&self, doc: &Document) -> bool {
        fn any(list: &[Entity], p: &HashSet<String>) -> bool {
            list.iter().any(|e| match e { Entity::Custom(c) => p.contains(c.type_.split('.').next().unwrap_or("")), Entity::Group(g) => any(&g.entities, p), _ => false })
        }
        any(&doc.entities, &self.prefixes) || doc.components.iter().any(|c| any(&c.entities, &self.prefixes)) || self.prefixes.iter().any(|p| doc.extensions.contains_key(p))
    }
    fn key(doc: &Document) -> (usize, u64, usize) { (doc as *const Document as usize, doc.version, doc.entities.len()) }
}

impl Plugin for WasmPlugin {
    fn id(&self) -> &str { &self.id }
    fn aliases(&self) -> &[&str] { &self.aliases }
    fn describe(&self) -> PluginInfo { self.info.clone() }
    fn exports(&self) -> Vec<ExportFormat> { self.exports.clone() }
    fn prepare(&self, doc: &Document) {
        let key = if self.owns(doc) && self.call(&Req { call: "prepare", doc: Some(doc), ..Default::default() }, FUEL).is_ok() { Some(Self::key(doc)) } else { None };
        *self.prepared.lock().unwrap() = key;
    }
    fn expand(&self, e: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError> {
        let sent = *self.prepared.lock().unwrap() == Some(Self::key(doc));
        let (v, _) = self.call(&Req { call: "expand", entity: Some(e), doc: if sent { None } else { Some(doc) }, ..Default::default() }, FUEL)?;
        serde_json::from_value(v).map_err(|e| OpError(format!("plugin {}: {e}", self.id)))
    }
    fn apply(&self, op: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        let (mut v, _) = self.call(&Req { call: "apply", op: Some(op), data: Some(&data), doc: Some(doc), ..Default::default() }, FUEL)?;
        let new: Document = serde_json::from_value(v["doc"].take()).map_err(|e| OpError(format!("plugin {}: {e}", self.id)))?;
        *doc = new;
        *self.prepared.lock().unwrap() = None;
        serde_json::from_value(v["result"].take()).map_err(|e| OpError(format!("plugin {}: {e}", self.id)))
    }
    fn transform(&self, e: &mut Custom, m: &Mat4, doc: &Document) -> Result<(), OpError> {
        let (v, _) = self.call(&Req { call: "transform", entity: Some(e), matrix: Some(m), doc: Some(doc), ..Default::default() }, FUEL)?;
        *e = serde_json::from_value(v).map_err(|x| OpError(format!("plugin {}: {x}", self.id)))?;
        Ok(())
    }
    fn paint(&self, e: &mut Custom, face_key: &str, material: &str, side: Side) -> Result<(), OpError> {
        let side = match side { Side::Front => "front", Side::Back => "back", Side::Both => "both" };
        let (v, _) = self.call(&Req { call: "paint", entity: Some(e), face_key: Some(face_key), material: Some(material), side: Some(side), ..Default::default() }, FUEL)?;
        *e = serde_json::from_value(v).map_err(|x| OpError(format!("plugin {}: {x}", self.id)))?;
        Ok(())
    }
    fn metrics(&self, doc: &Document) -> Option<Value> {
        if !self.owns(doc) { return None; }
        self.call(&Req { call: "metrics", doc: Some(doc), ..Default::default() }, FUEL).ok().map(|r| r.0).filter(|v| !v.is_null())
    }
    fn on_load(&self, doc: &mut Document) {
        if !self.owns(doc) { return; }
        match self.call(&Req { call: "on_load", doc: Some(doc), ..Default::default() }, FUEL) {
            Ok((v, _)) if !v.is_null() => { if let Ok(d) = serde_json::from_value(v) { *doc = d; } }
            Ok(_) => {}
            Err(e) => tracing::warn!("{}", e.0),
        }
    }
    fn export(&self, format: &str, scene: &CompiledScene, doc: &Document, options: &Value) -> Result<ExportFile, OpError> {
        let (v, bytes) = self.call(&Req { call: "export", format: Some(format), scene: Some(scene), doc: Some(doc), options: Some(options), ..Default::default() }, FUEL_EXPORT)?;
        Ok(ExportFile { bytes, report: v.get("report").cloned().unwrap_or(Value::Null) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plannr_core::{compile, CompileOptions, Op, Registry};
    use std::sync::Arc;

    /// The Car Track plugin's engine.wasm (from plannr-plugins: `cargo build --release --target wasm32-wasip1
    /// -p plannr-plugin-track`), named by PLANNR_TEST_TRACK_WASM; the tests are skipped without it.
    fn track_wasm() -> Option<Vec<u8>> { std::fs::read(std::env::var("PLANNR_TEST_TRACK_WASM").ok()?).ok() }

    #[test]
    fn a_wasm_plugin_runs_like_a_built_in_one() {
        let Some(bytes) = track_wasm() else { eprintln!("skipped: build the track wasm first"); return; };
        let w = WasmPlugin::load(&bytes).unwrap();
        assert_eq!(w.id(), "track");
        assert!(w.describe().ops.iter().any(|o| o.name == "add_piece"));
        let reg = Registry::new();
        reg.add(Arc::new(w), plannr_core::Source::Installed, true);
        let mut doc = Document::default(); doc.normalize();
        let ops: Vec<Op> = serde_json::from_value(serde_json::json!([
            { "op": "custom", "type": "track.add_piece", "data": { "kind": "straight", "length": 0.2 } },
            { "op": "custom", "type": "track.add_piece", "data": { "kind": "curve", "after": "last" } },
        ])).unwrap();
        let r = plannr_core::apply_many(&mut doc, &ops, &reg).unwrap();
        assert_eq!(r[1].created.len(), 1);
        let scene = compile(&doc, &reg, &CompileOptions::default());
        let tris: usize = scene.groups.iter().filter(|g| !g.back).map(|g| g.indices.len() / 3).sum();
        assert!(tris > 100, "{tris} triangles");
        assert_eq!(scene.metrics.plugins["track"]["pieces"], serde_json::json!(2));
        // moving the first piece through the sandbox moves the chain
        let first = doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.data.get("prev").is_none() => Some(c.id.clone()), _ => None }).unwrap();
        let mv: Vec<Op> = serde_json::from_value(serde_json::json!([{ "op": "move", "ids": [first], "delta": [1, 0, 0] }])).unwrap();
        plannr_core::apply_many(&mut doc, &mv, &reg).unwrap();
        let scene2 = compile(&doc, &reg, &CompileOptions::default());
        assert!((scene2.bounds.min[0] - scene.bounds.min[0] - 1.0).abs() < 1e-3);
        // switched off: nothing drawn, ops refused; back on: all there again
        reg.set_enabled("track", false);
        assert!(compile(&doc, &reg, &CompileOptions::default()).groups.is_empty());
        assert!(plannr_core::apply_many(&mut doc, &ops[..1], &reg).is_err());
        reg.set_enabled("track", true);
        assert!(!compile(&doc, &reg, &CompileOptions::default()).groups.is_empty());
    }

    #[test]
    fn a_trap_is_an_error_and_the_plugin_recovers() {
        let Some(bytes) = track_wasm() else { return; };
        let w = WasmPlugin::load(&bytes).unwrap();
        let mut doc = Document::default();
        assert!(w.apply("track.no_such_op", serde_json::json!({}), &mut doc).is_err());
        assert!(w.apply("track.add_piece", serde_json::json!({ "kind": "straight" }), &mut doc).is_ok());
    }
}
