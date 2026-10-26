//! Parametric component scripts: Studio's `component(id, { params, rules, build })` JavaScript,
//! run in an embedded Boa engine. A script says *what shapes* (prisms: a plan outline between
//! two heights) in the component's own frame; placing them in the house happens in `expand`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use boa_engine::{Context, JsString, JsValue, Source};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::catalogue;
use crate::data::{ComponentType, Settings, Unit, P2};

/// One declared parameter, after the type's rules have resolved the given values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Param {
    pub name: String,
    pub label: String,
    /// number | length | integer | choice | flag | derived
    pub kind: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default)]
    pub choices: Vec<String>,
}

/// A lump a script asked for: a plan outline (local x across, y forward) between two heights.
#[derive(Debug, Clone, PartialEq)]
pub struct Solid {
    pub plan: Vec<P2>,
    pub low: f64,
    pub high: f64,
    pub material: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meta {
    pub name: String,
    pub needs: Vec<String>,
    pub wants: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Worktop { pub at: f64, pub depth: f64 }

/// Local bounding box of a set of solids: (min x, max x, min y_forward, max y_forward, low, high).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds { pub min_x: f64, pub max_x: f64, pub min_d: f64, pub max_d: f64, pub low: f64, pub high: f64 }
impl Bounds {
    pub fn width(&self) -> f64 { self.max_x - self.min_x }
    pub fn depth(&self) -> f64 { self.max_d - self.min_d }
    pub fn height(&self) -> f64 { self.high - self.low }
}
pub fn bounds(solids: &[Solid]) -> Option<Bounds> {
    let mut b: Option<Bounds> = None;
    for s in solids {
        for p in &s.plan {
            b = Some(match b {
                None => Bounds { min_x: p[0], max_x: p[0], min_d: p[1], max_d: p[1], low: s.low, high: s.high },
                Some(o) => Bounds { min_x: o.min_x.min(p[0]), max_x: o.max_x.max(p[0]), min_d: o.min_d.min(p[1]), max_d: o.max_d.max(p[1]), low: o.low.min(s.low), high: o.high.max(s.high) },
            });
        }
    }
    b
}

/// Material names a script may use (Studio's), with their colours; anything else that is not a
/// `#RRGGBB` colour falls back to Carcass, as in Studio.
pub const C_MATERIALS: &[(&str, &str, f64)] = &[
    ("Carcass", "#EDECE8", 1.0), ("Front", "#F7F6F3", 1.0), ("Worktop", "#474A4E", 1.0), ("Handle", "#9AA0A6", 1.0),
    ("Appliance", "#22242A", 1.0), ("Sanitary", "#FBFBFA", 1.0), ("Glass", "#D8E6EC", 0.4), ("ShowerGlass", "#D8E6EC", 0.4),
    ("Radiator", "#F4F5F6", 1.0), ("Timber", "#C8B49A", 1.0), ("Frame", "#1E1F22", 1.0), ("Steel", "#8E9398", 1.0),
];
fn is_hex(s: &str) -> bool { s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit()) }
/// Material id for a script material name: `construction.c.<Name>`, `construction.c.#RRGGBB` or Carcass.
pub fn material_for(name: &str) -> String {
    if is_hex(name) { return format!("construction.c.{}", name.to_uppercase()); }
    if C_MATERIALS.iter().any(|(n, ..)| *n == name) { format!("construction.c.{name}") } else { "construction.c.Carcass".into() }
}
/// (id, display name, colour, opacity) for a script material name, for creating the material.
pub fn material_spec(name: &str) -> (String, String, String, f64) {
    if is_hex(name) { let up = name.to_uppercase(); return (format!("construction.c.{up}"), up.clone(), up, 1.0); }
    let (n, hex, op) = C_MATERIALS.iter().find(|(n, ..)| *n == name).copied().unwrap_or(("Carcass", "#EDECE8", 1.0));
    (format!("construction.c.{n}"), format!("Unit · {n}"), hex.to_string(), op)
}

/// The built-in types, parsed once.
pub fn builtin_types() -> &'static [ComponentType] {
    static B: OnceLock<Vec<ComponentType>> = OnceLock::new();
    B.get_or_init(catalogue::builtin)
}
/// Every type a document can use: its own first, then the built-ins it does not override.
pub fn available_types(s: &Settings) -> Vec<ComponentType> {
    let mut out = s.component_types.clone();
    for b in builtin_types() { if !out.iter().any(|t| t.id == b.id) { out.push(b.clone()); } }
    out
}
pub fn find_type<'a>(types: &'a [ComponentType], id: &str) -> Option<&'a ComponentType> { types.iter().find(|t| t.id == id) }

/// The values + choices of a unit as the object a script's rules receive.
pub fn given(u: &Unit) -> Value {
    let mut m = Map::new();
    for (k, v) in &u.values { m.insert(k.clone(), v.clone()); }
    for (k, v) in &u.choices { m.insert(k.clone(), v.clone()); }
    Value::Object(m)
}
fn given_key(type_: &str, given: &Value) -> String {
    let mut k = type_.to_string();
    if let Some(o) = given.as_object() {
        let mut pairs: Vec<_> = o.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        for (n, v) in pairs { k.push('\u{1}'); k.push_str(n); k.push('='); k.push_str(&v.to_string()); }
    }
    k
}

fn fingerprint(types: &[ComponentType]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for t in types { t.id.hash(&mut h); t.source.hash(&mut h); }
    h.finish()
}

/// One Boa context with the prelude and a set of types declared, plus caches of what they answered.
pub struct Runtime {
    ctx: Context,
    declared: Vec<String>,
    problems: Vec<String>,
    solids: HashMap<String, Result<Vec<Solid>, String>>,
    params: HashMap<String, Result<Vec<Param>, String>>,
    worktops: HashMap<String, Option<Worktop>>,
    metas: HashMap<String, Option<Meta>>,
    pub calls: usize,
}

fn new_context() -> Context {
    let mut ctx = Context::default();
    let l = ctx.runtime_limits_mut();
    l.set_loop_iteration_limit(2_000_000);
    l.set_recursion_limit(200);
    ctx
}
fn eval(ctx: &mut Context, src: &str) -> Result<JsValue, String> {
    ctx.eval(Source::from_bytes(src.as_bytes())).map_err(|e| e.to_string())
}

impl Runtime {
    pub fn new(types: &[ComponentType]) -> Runtime {
        let mut ctx = new_context();
        let mut problems = vec![];
        if let Err(e) = eval(&mut ctx, catalogue::PRELUDE) { problems.push(format!("prelude: {e}")); }
        for t in types {
            if let Err(e) = eval(&mut ctx, &t.source) { problems.push(format!("{}: {e}", t.id)); }
        }
        let mut rt = Runtime { ctx, declared: vec![], problems, solids: HashMap::new(), params: HashMap::new(), worktops: HashMap::new(), metas: HashMap::new(), calls: 0 };
        rt.declared = match eval(&mut rt.ctx, "Object.keys(__types)").and_then(|v| v.to_json(&mut rt.ctx).map_err(|e| e.to_string())) {
            Ok(Some(Value::Array(a))) => a.into_iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
            _ => vec![],
        };
        rt
    }
    pub fn problems(&self) -> &[String] { &self.problems }
    pub fn declared(&self) -> &[String] { &self.declared }
    pub fn has(&self, type_: &str) -> bool { self.declared.iter().any(|d| d == type_) }

    fn call(&mut self, func: &str, type_: &str, given: &Value) -> Result<Value, String> {
        self.calls += 1;
        let ctx = &mut self.ctx;
        let f = ctx.global_object().get(JsString::from(func), ctx).map_err(|e| e.to_string())?;
        let callable = f.as_callable().ok_or_else(|| format!("{func} is not a function"))?.clone();
        let g = JsValue::from_json(given, ctx).map_err(|e| e.to_string())?;
        let out = callable.call(&JsValue::undefined(), &[JsValue::from(JsString::from(type_)), g], ctx).map_err(|e| e.to_string())?;
        Ok(out.to_json(ctx).map_err(|e| e.to_string())?.unwrap_or(Value::Null))
    }

    /// The parameters of a type after its rules have seen the given values.
    pub fn params(&mut self, type_: &str, given: &Value) -> Result<Vec<Param>, String> {
        let key = given_key(type_, given);
        if let Some(r) = self.params.get(&key) { return r.clone(); }
        let r = self.call("__params", type_, given).and_then(|v| serde_json::from_value::<Vec<Param>>(v).map_err(|e| e.to_string()));
        self.params.insert(key, r.clone());
        r
    }

    /// The solids a type builds for the given values (cached per type + values).
    pub fn solids(&mut self, type_: &str, given: &Value) -> Result<Vec<Solid>, String> {
        let key = given_key(type_, given);
        if let Some(r) = self.solids.get(&key) { return r.clone(); }
        let r = self.call("__solids", type_, given).map(|v| {
            v.as_array().map(|rows| rows.iter().filter_map(|row| {
                let plan: Vec<P2> = row.get("plan")?.as_array()?.iter().filter_map(|p| Some([p.get("x")?.as_f64()?, p.get("y")?.as_f64()?])).collect();
                let (low, high) = (row.get("low")?.as_f64()?, row.get("high")?.as_f64()?);
                if plan.len() < 3 || !(high > low) || plan.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) { return None; }
                Some(Solid { plan, low, high, material: row.get("material").and_then(|m| m.as_str()).unwrap_or("Carcass").to_string() })
            }).collect()).unwrap_or_default()
        });
        self.solids.insert(key, r.clone());
        r
    }

    pub fn meta(&mut self, type_: &str, given: &Value) -> Option<Meta> {
        let key = given_key(type_, given);
        if let Some(m) = self.metas.get(&key) { return m.clone(); }
        let m = self.call("__meta", type_, given).ok().and_then(|v| if v.is_object() { Some(Meta {
            name: v.get("name").and_then(|x| x.as_str()).unwrap_or(type_).to_string(),
            needs: v.get("needs").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
            wants: v.get("wants").and_then(|x| x.as_f64()).unwrap_or(0.0),
            symbol: v.get("symbol").and_then(|x| x.as_str()).map(|s| s.to_string()),
        }) } else { None });
        self.metas.insert(key, m.clone());
        m
    }

    pub fn worktop(&mut self, type_: &str, given: &Value) -> Option<Worktop> {
        let key = given_key(type_, given);
        if let Some(w) = self.worktops.get(&key) { return *w; }
        let w = self.call("__worktop", type_, given).ok().and_then(|v| Some(Worktop { at: v.get("at")?.as_f64()?, depth: v.get("depth")?.as_f64()? }));
        self.worktops.insert(key, w);
        w
    }
}

thread_local! {
    // ManuallyDrop: Boa's garbage collector lives in its own thread-local, and the order in which
    // thread-locals are torn down at thread exit is unspecified — dropping a Context after the
    // collector is gone aborts. A stale runtime is dropped explicitly (while the thread runs); the
    // last one per thread is leaked on purpose.
    static RUNTIME: RefCell<Option<(u64, std::mem::ManuallyDrop<Runtime>)>> = const { RefCell::new(None) };
}

/// Run `f` with the runtime for these types; the runtime is kept per thread and rebuilt only when
/// the set of type sources changes.
pub fn with_runtime<R>(types: &[ComponentType], f: impl FnOnce(&mut Runtime) -> R) -> R {
    let fp = fingerprint(types);
    RUNTIME.with(|cell| {
        let mut slot = cell.borrow_mut();
        let stale = slot.as_ref().map(|(k, _)| *k != fp).unwrap_or(true);
        if stale {
            if let Some((_, mut old)) = slot.take() { unsafe { std::mem::ManuallyDrop::drop(&mut old); } }
            *slot = Some((fp, std::mem::ManuallyDrop::new(Runtime::new(types))));
        }
        f(&mut slot.as_mut().unwrap().1)
    })
}

/// Solids of a unit, or None when its type has no script or the script failed (then the box is used).
pub fn unit_solids(rt: &mut Runtime, u: &Unit) -> Option<Vec<Solid>> {
    if u.type_.is_empty() || !rt.has(&u.type_) { return None; }
    match rt.solids(&u.type_, &given(u)) {
        Ok(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Check a script in a scratch runtime: it must declare `id` and build without throwing.
pub fn validate_source(id: &str, source: &str) -> Result<(Vec<Param>, Vec<Solid>), String> {
    let t = ComponentType { id: id.to_string(), name: id.to_string(), group: String::new(), source: source.to_string() };
    let mut rt = Runtime::new(std::slice::from_ref(&t));
    if let Some(p) = rt.problems().first() { return Err(p.clone()); }
    if !rt.has(id) { return Err(format!("the script must call component('{id}', {{ … }}); it declared {:?}", rt.declared())); }
    let params = rt.params(id, &json!({}))?;
    let solids = rt.solids(id, &json!({}))?;
    if solids.is_empty() { return Err("the script built no solids for its default parameters".into()); }
    Ok((params, solids))
}
