//! Thermal transmittance (U-value) of masonry wall build-ups, the simple way (NBN B 62-002 style:
//! R = Rsi + Σ d/λ + R_cavity + Rse, U = 1/R, no corrections for ties or thermal bridges).

use plannr_core::model::Document;
use plannr_core::ops::OpError;
use serde_json::{json, Value};

use crate::masonry::data::*;

pub const RSI: f64 = 0.13;
pub const RSE: f64 = 0.04;
/// EPB requirement for walls in renovation (Flanders, U_max in W/m²K).
pub const U_MAX: f64 = 0.24;
/// Thermal resistance of an unventilated cavity of 25–150 mm (m²K/W).
pub const R_CAVITY: f64 = 0.17;

/// Design thermal conductivity λ (W/mK) of a unit kind.
pub fn lambda_unit(kind: &str) -> f64 {
    match kind { "facingBrick" => 0.9, "brick" => 0.7, "clayBlock" => 0.27, "concreteBlock" => 1.1, "aeratedBlock" => 0.16, "limeSandBlock" => 1.0, _ => 0.7 }
}
/// λ (W/mK) of an insulation material.
pub fn lambda_insulation(material: &str) -> f64 {
    match material { "eps" => 0.034, "pur" => 0.026, "mineralWool" => 0.035, "pir" => 0.022, "xps" => 0.034, _ => 0.035 }
}
pub const LAMBDA_PLASTER: f64 = 0.57;

#[derive(Debug, Clone)]
pub struct Layer { pub name: String, pub thickness: f64, pub lambda: Option<f64>, pub r: f64 }

/// Layers from inside to outside with their resistances, and the total R and U.
pub fn layers(w: &Wall, d: &Defaults) -> (Vec<Layer>, f64, f64) {
    let mut v = vec![Layer { name: "Rsi (inside surface)".into(), thickness: 0.0, lambda: None, r: RSI }];
    if w.plaster.interior { v.push(Layer { name: "Plaster".into(), thickness: w.plaster.thickness, lambda: Some(LAMBDA_PLASTER), r: w.plaster.thickness / LAMBDA_PLASTER }); }
    let inner = w.inner_unit(d);
    let li = lambda_unit(&inner.kind);
    v.push(Layer { name: if inner.name.is_empty() { inner.kind.clone() } else { inner.name.clone() }, thickness: inner.width, lambda: Some(li), r: inner.width / li });
    let mut rse = RSE;
    if w.is_cavity() {
        let cav = w.cavity_of(d);
        let ins = &cav.insulation;
        match ins.kind.as_str() {
            "fill" => { let l = lambda_insulation(&ins.material); v.push(Layer { name: format!("Cavity fill ({})", ins.material), thickness: cav.width, lambda: Some(l), r: cav.width / l }); }
            "board" => {
                let t = ins.thickness.min(cav.width).max(0.0); let l = lambda_insulation(&ins.material);
                v.push(Layer { name: format!("Insulation board ({})", ins.material), thickness: t, lambda: Some(l), r: t / l });
                // the air gap in front of a board is ventilated through the outer leaf: the outer leaf no longer counts and Rse becomes Rsi-like
                if cav.width - t > 0.005 { v.push(Layer { name: "Ventilated air gap".into(), thickness: cav.width - t, lambda: None, r: 0.0 }); }
                rse = RSI;
            }
            _ => { if cav.width >= 0.005 { v.push(Layer { name: "Unventilated cavity (air)".into(), thickness: cav.width, lambda: None, r: if cav.width >= 0.025 { R_CAVITY } else { 0.11 } }); } }
        }
        let outer = w.outer_unit(d);
        let lo = lambda_unit(&outer.kind);
        let counts = rse == RSE;
        v.push(Layer { name: if outer.name.is_empty() { outer.kind.clone() } else { outer.name.clone() }, thickness: outer.width, lambda: Some(lo), r: if counts { outer.width / lo } else { 0.0 } });
    }
    if w.plaster.exterior { v.push(Layer { name: if w.is_cavity() { "Exterior render".into() } else { "Plaster (other side)".into() }, thickness: w.plaster.thickness, lambda: Some(LAMBDA_PLASTER), r: w.plaster.thickness / LAMBDA_PLASTER }); }
    v.push(Layer { name: "Rse (outside surface)".into(), thickness: 0.0, lambda: None, r: rse });
    let total: f64 = v.iter().map(|l| l.r).sum();
    (v, total, 1.0 / total)
}

fn r3(x: f64) -> f64 { (x * 1000.0).round() / 1000.0 }

/// The U-value report for one wall.
pub fn report(w: &Wall, d: &Defaults) -> Value {
    let (ls, r, u) = layers(w, d);
    let mut out = json!({
        "type": w.type_, "layers": ls.iter().map(|l| json!({ "name": l.name, "thickness": r3(l.thickness), "lambda": l.lambda, "R": r3(l.r) })).collect::<Vec<_>>(),
        "R_total": r3(r), "U": r3(u), "requirement": U_MAX, "meets": u <= U_MAX + 1e-9,
    });
    // a single leaf plastered on both sides is an interior partition: no EPB requirement
    if !w.is_cavity() && w.plaster.interior && w.plaster.exterior { out["partition"] = json!(true); out["requirement"] = Value::Null; out["meets"] = Value::Null; }
    if w.is_cavity() && w.cavity_of(d).insulation.kind == "none" {
        // what a full cavity fill would give (spouwmuurisolatie)
        let mut alt = vec![];
        for m in ["eps", "pur", "mineralWool"] {
            let mut w2 = w.clone();
            let mut cav = w.cavity_of(d); cav.insulation = Insulation { kind: "fill".into(), material: m.into(), thickness: cav.width };
            w2.cavity = Some(cav);
            let (_, _, u2) = layers(&w2, d);
            alt.push(json!({ "material": m, "U": r3(u2) }));
        }
        out["withFill"] = json!(alt);
    }
    out
}

/// A key that groups walls with the same build-up.
pub fn buildup_key(w: &Wall, d: &Defaults) -> String {
    let i = w.inner_unit(d);
    if w.is_cavity() {
        let o = w.outer_unit(d); let c = w.cavity_of(d);
        format!("cavity|{}|{}|{}|{}|{}|{}|{}", o.kind, o.width, c.width, c.insulation.kind, c.insulation.material, c.insulation.thickness, format_args!("{}|{}|{}", i.kind, i.width, w.plaster.interior))
    } else { format!("single|{}|{}|{}", i.kind, i.width, w.plaster.interior) }
}

pub fn uvalue_op(doc: &Document, data: &Value) -> Result<Value, OpError> {
    let h = House::read(doc);
    if let Some(id) = data.get("wall").and_then(|v| v.as_str()) {
        let (_, w) = h.walls.iter().find(|(i, _)| i == id).ok_or_else(|| OpError(format!("no masonry wall {id}")))?;
        let mut r = report(w, &h.defaults); r["wall"] = json!(id);
        return Ok(r);
    }
    // one entry per distinct build-up, with the walls that share it
    let mut groups: Vec<(String, Vec<String>, Value, f64)> = vec![];
    for (id, w) in &h.walls {
        let k = buildup_key(w, &h.defaults);
        let area = w.length() * h.height(w.level);
        match groups.iter_mut().find(|g| g.0 == k) { Some(g) => { g.1.push(id.clone()); g.3 += area; } None => groups.push((k, vec![id.clone()], report(w, &h.defaults), area)) }
    }
    Ok(json!({ "buildups": groups.into_iter().map(|(_, ids, mut r, area)| { r["walls"] = json!(ids); r["area_m2"] = json!((area * 10.0).round() / 10.0); r }).collect::<Vec<_>>(), "requirement": U_MAX }))
}
