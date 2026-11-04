//! The plumbing catalogue: systems, pipe materials with their diameter series, joint types,
//! fitting kinds, bracket rules. Belgian / Dutch residential practice (Alupex press and clamp
//! fittings, copper, PVC drains, PE), metric: `dn` is always the outer diameter in millimetres.

use serde_json::{json, Value};

pub const PLUGIN_ID: &str = "plumbing";
pub const T_PIPE: &str = "plumbing.pipe";
pub const T_FITTING: &str = "plumbing.fitting";
pub const T_BRACKET: &str = "plumbing.bracket";

/// (id, label, Dutch label, colour)
pub const SYSTEMS: [(&str, &str, &str, &str); 7] = [
    ("coldWater", "Cold water", "Koud water", "#2E7BE4"),
    ("hotWater", "Hot water", "Warm water", "#E5484D"),
    ("heatingFlow", "Heating flow", "CV aanvoer", "#F2711C"),
    ("heatingReturn", "Heating return", "CV retour", "#1B9AAA"),
    ("drain", "Drain / waste", "Afvoer", "#8A8F98"),
    ("rainwater", "Rainwater", "Regenwater", "#3BA55D"),
    ("gas", "Gas", "Gas", "#E8C62C"),
];
pub fn system_ids() -> Vec<&'static str> { SYSTEMS.iter().map(|s| s.0).collect() }
pub fn system_label(id: &str) -> &'static str { SYSTEMS.iter().find(|s| s.0 == id).map(|s| s.1).unwrap_or("Unknown") }
pub fn system_color(id: &str) -> &'static str { SYSTEMS.iter().find(|s| s.0 == id).map(|s| s.3).unwrap_or("#888888") }
pub fn is_drain(system: &str) -> bool { matches!(system, "drain" | "rainwater") }

/// A pipe material (what the pipe is made of, not a plannr material).
pub struct PipeMaterial {
    pub id: &'static str,
    pub label: &'static str,
    pub label_nl: &'static str,
    /// Outer diameters in mm that exist (first = default).
    pub dns: &'static [f64],
    /// Wall thickness in mm per dn (same order), for labels like 16×2.
    pub walls: &'static [f64],
    /// Joint types that exist for it (first = default).
    pub joints: &'static [&'static str],
    /// Systems it may carry.
    pub systems: &'static [&'static str],
    /// Largest dn that is bent instead of fitted with elbows (0 = never bent).
    pub bend_up_to: f64,
    /// Minimum bend radius as a multiple of the outer diameter.
    pub min_bend: f64,
    /// Rendered as this plannr material; `None` = the system colour.
    pub look: Option<&'static str>,
    /// Bracket spacing in m for horizontal runs: (dn upper bound, spacing); vertical = ×1.6.
    pub spacing: &'static [(f64, f64)],
}

pub const MATERIALS: [PipeMaterial; 6] = [
    PipeMaterial { id: "alupex", label: "Alupex (multilayer PE-RT/Al)", label_nl: "Alupex meerlagenbuis", dns: &[16.0, 20.0, 26.0, 32.0, 14.0, 40.0, 50.0, 63.0], walls: &[2.0, 2.0, 3.0, 3.0, 2.0, 3.5, 4.0, 4.5], joints: &["press", "clamp", "pushfit"], systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn", "gas"], bend_up_to: 26.0, min_bend: 5.0, look: None, spacing: &[(16.0, 0.5), (20.0, 0.6), (26.0, 0.8), (32.0, 1.0), (1000.0, 1.2)] },
    PipeMaterial { id: "pex", label: "PEX (in sleeve)", label_nl: "PEX / VPE in mantelbuis", dns: &[16.0, 20.0, 25.0], walls: &[2.2, 2.8, 3.5], joints: &["press", "clamp", "sleeve"], systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn"], bend_up_to: 25.0, min_bend: 6.0, look: None, spacing: &[(16.0, 0.4), (20.0, 0.5), (1000.0, 0.6)] },
    PipeMaterial { id: "copper", label: "Copper", label_nl: "Koper", dns: &[15.0, 12.0, 18.0, 22.0, 28.0, 10.0, 35.0, 42.0], walls: &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.5, 1.5], joints: &["solder", "press", "compression"], systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn", "gas"], bend_up_to: 0.0, min_bend: 3.5, look: Some("copper"), spacing: &[(12.0, 1.0), (15.0, 1.25), (22.0, 1.5), (28.0, 2.0), (1000.0, 2.5)] },
    PipeMaterial { id: "pvc", label: "PVC (drain)", label_nl: "PVC afvoerbuis", dns: &[40.0, 32.0, 50.0, 75.0, 90.0, 110.0, 125.0, 160.0], walls: &[3.0, 3.0, 3.0, 3.0, 3.0, 3.2, 3.2, 4.0], joints: &["glue", "ring"], systems: &["drain", "rainwater"], bend_up_to: 0.0, min_bend: 0.0, look: Some("pvc"), spacing: &[(50.0, 0.5), (90.0, 0.8), (125.0, 1.1), (1000.0, 1.5)] },
    PipeMaterial { id: "pe", label: "PE (HDPE drain)", label_nl: "PE afvoerbuis", dns: &[50.0, 40.0, 56.0, 63.0, 75.0, 90.0, 110.0, 125.0, 160.0], walls: &[3.0, 3.0, 3.0, 3.0, 3.0, 3.5, 4.2, 4.8, 6.2], joints: &["weld"], systems: &["drain", "rainwater"], bend_up_to: 0.0, min_bend: 0.0, look: Some("pe"), spacing: &[(63.0, 0.6), (110.0, 0.9), (1000.0, 1.2)] },
    PipeMaterial { id: "steel", label: "Steel (threaded, galvanised)", label_nl: "Gegalvaniseerd staal (draad)", dns: &[21.3, 26.9, 33.7, 42.4], walls: &[2.6, 2.6, 3.2, 3.2], joints: &["thread"], systems: &["coldWater", "heatingFlow", "heatingReturn", "gas"], bend_up_to: 0.0, min_bend: 0.0, look: Some("steel"), spacing: &[(21.3, 2.0), (26.9, 2.5), (1000.0, 3.0)] },
];
pub fn material(id: &str) -> Option<&'static PipeMaterial> { MATERIALS.iter().find(|m| m.id == id) }
pub fn material_ids() -> Vec<&'static str> { MATERIALS.iter().map(|m| m.id).collect() }
/// Default pipe material for a system.
pub fn default_material(system: &str) -> &'static str { if is_drain(system) { "pvc" } else { "alupex" } }
pub fn dn_label(mat: &PipeMaterial, dn: f64) -> String {
    let w = mat.dns.iter().position(|d| (d - dn).abs() < 1e-6).and_then(|i| mat.walls.get(i)).copied();
    if mat.id == "steel" { return thread_of_steel(dn).to_string(); }
    match w { Some(w) => format!("{}×{}", num(dn), num(w)), None => num(dn) }
}
pub fn num(v: f64) -> String { if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.1}") } }
pub fn thread_of_steel(dn: f64) -> &'static str { if dn < 22.0 { "1/2\"" } else if dn < 28.0 { "3/4\"" } else if dn < 35.0 { "1\"" } else { "1 1/4\"" } }
/// Horizontal bracket spacing in m for a material and dn.
pub fn bracket_spacing(mat: &PipeMaterial, dn: f64) -> f64 { mat.spacing.iter().find(|(up, _)| dn <= *up + 1e-6).map(|x| x.1).unwrap_or(1.0) }
/// Default corner style: bends while the material allows it, elbows otherwise.
pub fn default_corners(mat: &PipeMaterial, dn: f64) -> &'static str { if dn <= mat.bend_up_to { "bend" } else { "elbow" } }
/// Minimum bend radius (m) of a pipe that is bent; `None` when the material is never bent.
pub fn min_bend_radius(mat: &PipeMaterial, dn: f64) -> Option<f64> { if mat.min_bend > 0.0 { Some(mat.min_bend * dn / 1000.0) } else { None } }

/// (id, label, Dutch label)
pub const JOINTS: [(&str, &str, &str); 10] = [
    ("press", "Press fitting", "Perskoppeling"),
    ("clamp", "Clamp (compression) fitting", "Klemkoppeling / schroefkoppeling"),
    ("pushfit", "Push-fit", "Steekkoppeling"),
    ("sleeve", "Sliding sleeve", "Schuifhuls"),
    ("solder", "Soldered (capillary)", "Soldeer"),
    ("compression", "Compression (olive)", "Knelkoppeling"),
    ("glue", "Solvent weld", "Lijmverbinding"),
    ("ring", "Rubber ring socket", "Manchetmof"),
    ("weld", "Welded (PE)", "Las (elektrolas / spiegellas)"),
    ("thread", "Threaded", "Draadkoppeling"),
];
pub fn joint_label(id: &str) -> &'static str { JOINTS.iter().find(|j| j.0 == id).map(|j| j.1).unwrap_or(id_static(id)) }
fn id_static(_id: &str) -> &'static str { "Joint" }
pub fn joint_ids() -> Vec<&'static str> { JOINTS.iter().map(|j| j.0).collect() }

/// How many ports a fitting has and what they are called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape { Inline, Elbow, Tee, End, Mixer, Manifold, Trap, WallPlate }

pub struct FittingKind {
    pub id: &'static str,
    pub label: &'static str,
    pub label_nl: &'static str,
    pub shape: Shape,
    /// Systems it belongs to (empty = any).
    pub systems: &'static [&'static str],
    /// Second diameter (branch / outlet) is meaningful.
    pub has_dn2: bool,
    /// Drawn in chrome / brass as sanitary tapware rather than as a fitting of the pipe's joint type.
    pub tapware: bool,
}

pub const FITTINGS: [FittingKind; 23] = [
    FittingKind { id: "coupling", label: "Straight coupling", label_nl: "Rechte koppeling", shape: Shape::Inline, systems: &[], has_dn2: false, tapware: false },
    FittingKind { id: "elbow90", label: "Elbow 90°", label_nl: "Knie 90°", shape: Shape::Elbow, systems: &[], has_dn2: false, tapware: false },
    FittingKind { id: "elbow45", label: "Elbow 45°", label_nl: "Bocht 45°", shape: Shape::Elbow, systems: &[], has_dn2: false, tapware: false },
    FittingKind { id: "tee", label: "Tee", label_nl: "T-stuk", shape: Shape::Tee, systems: &[], has_dn2: true, tapware: false },
    FittingKind { id: "reducer", label: "Reducer", label_nl: "Verloopstuk", shape: Shape::Inline, systems: &[], has_dn2: true, tapware: false },
    FittingKind { id: "adapter", label: "Threaded adapter", label_nl: "Overgangsstuk met draad", shape: Shape::Inline, systems: &[], has_dn2: false, tapware: false },
    FittingKind { id: "cap", label: "End cap", label_nl: "Eindstop", shape: Shape::End, systems: &[], has_dn2: false, tapware: false },
    FittingKind { id: "wallPlate", label: "Wall plate elbow (½\")", label_nl: "Muurplaat", shape: Shape::WallPlate, systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn", "gas"], has_dn2: false, tapware: false },
    FittingKind { id: "stopValve", label: "Stop valve", label_nl: "Stopkraan", shape: Shape::Inline, systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn"], has_dn2: false, tapware: false },
    FittingKind { id: "ballValve", label: "Ball valve", label_nl: "Kogelkraan", shape: Shape::Inline, systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn", "gas"], has_dn2: false, tapware: false },
    FittingKind { id: "checkValve", label: "Check valve", label_nl: "Terugslagklep", shape: Shape::Inline, systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn"], has_dn2: false, tapware: false },
    FittingKind { id: "pressureReducer", label: "Pressure reducer", label_nl: "Drukregelaar", shape: Shape::Inline, systems: &["coldWater"], has_dn2: false, tapware: false },
    FittingKind { id: "filter", label: "Filter", label_nl: "Filter", shape: Shape::Inline, systems: &["coldWater", "heatingFlow", "heatingReturn"], has_dn2: false, tapware: false },
    FittingKind { id: "meter", label: "Water meter", label_nl: "Watermeter", shape: Shape::Inline, systems: &["coldWater"], has_dn2: false, tapware: false },
    FittingKind { id: "angleValve", label: "Angle stop valve (½\" × ⅜\")", label_nl: "Hoekstopkraan", shape: Shape::Elbow, systems: &["coldWater", "hotWater"], has_dn2: false, tapware: true },
    FittingKind { id: "radiatorValve", label: "Radiator valve (thermostatic)", label_nl: "Radiatorkraan", shape: Shape::Elbow, systems: &["heatingFlow", "heatingReturn"], has_dn2: false, tapware: false },
    FittingKind { id: "mixerWall", label: "Wall mixer tap (150 mm)", label_nl: "Wandmengkraan", shape: Shape::Mixer, systems: &["coldWater", "hotWater"], has_dn2: false, tapware: true },
    FittingKind { id: "mixerThermo", label: "Thermostatic shower mixer", label_nl: "Thermostaatkraan", shape: Shape::Mixer, systems: &["coldWater", "hotWater"], has_dn2: false, tapware: true },
    FittingKind { id: "mixerBasin", label: "Basin mixer (single lever)", label_nl: "Wastafelmengkraan", shape: Shape::Mixer, systems: &["coldWater", "hotWater"], has_dn2: false, tapware: true },
    FittingKind { id: "manifold", label: "Manifold", label_nl: "Verdeler", shape: Shape::Manifold, systems: &["coldWater", "hotWater", "heatingFlow", "heatingReturn"], has_dn2: true, tapware: false },
    FittingKind { id: "trap", label: "Bottle trap", label_nl: "Sifon", shape: Shape::Trap, systems: &["drain"], has_dn2: false, tapware: true },
    FittingKind { id: "cleanout", label: "Access piece", label_nl: "Ontstoppingsstuk", shape: Shape::Inline, systems: &["drain", "rainwater"], has_dn2: false, tapware: false },
    FittingKind { id: "branch45", label: "Branch 45°", label_nl: "T-stuk 45°", shape: Shape::Tee, systems: &["drain", "rainwater"], has_dn2: true, tapware: false },
];
pub fn fitting(id: &str) -> Option<&'static FittingKind> { FITTINGS.iter().find(|f| f.id == id) }
pub fn fitting_ids() -> Vec<&'static str> { FITTINGS.iter().map(|f| f.id).collect() }

pub const BRACKET_KINDS: [(&str, &str, &str); 3] = [
    ("clip", "Pipe clip", "Buisclip"),
    ("clamp", "Clamp with rubber inlay and threaded rod", "Beugel met rubber inlage en stokschroef"),
    ("double", "Double clip (flow + return)", "Dubbele buisclip"),
];
pub fn bracket_kind_ids() -> Vec<&'static str> { BRACKET_KINDS.iter().map(|b| b.0).collect() }

/// Plannr materials the plugin renders with: (key, name, colour, opacity).
pub const LOOKS: [(&str, &str, &str, f64); 20] = [
    ("coldWater", "Cold water pipe", "#2E7BE4", 1.0), ("hotWater", "Hot water pipe", "#E5484D", 1.0),
    ("heatingFlow", "Heating flow pipe", "#F2711C", 1.0), ("heatingReturn", "Heating return pipe", "#1B9AAA", 1.0),
    ("drain", "Drain pipe", "#8A8F98", 1.0), ("rainwater", "Rainwater pipe", "#3BA55D", 1.0), ("gas", "Gas pipe", "#E8C62C", 1.0),
    ("copper", "Copper", "#B87333", 1.0), ("pvc", "PVC (grey)", "#8A8F98", 1.0), ("pe", "PE (black)", "#2A2A2A", 1.0), ("steel", "Galvanised steel", "#7B8290", 1.0),
    ("brass", "Brass fitting", "#C9A227", 1.0), ("press", "Press sleeve (stainless)", "#B4B8BE", 1.0), ("pressRing", "Press indicator ring", "#2B2B2B", 1.0),
    ("pvcFitting", "PVC fitting", "#6F747C", 1.0), ("chrome", "Chrome", "#D9DDE2", 1.0), ("handle", "Valve handle", "#D93025", 1.0),
    ("bracket", "Bracket (galvanised)", "#9AA0A6", 1.0), ("rubber", "Rubber inlay", "#333333", 1.0), ("insulation", "Pipe insulation", "#D7D2C8", 0.45),
];
pub fn look_id(key: &str) -> String { format!("plumbing.{key}") }
pub fn all_look_ids() -> Vec<String> { LOOKS.iter().map(|l| look_id(l.0)).collect() }

/// The catalogue as JSON (the `catalog` op), so an MCP client can see what exists.
pub fn catalog_json() -> Value {
    json!({
        "systems": SYSTEMS.iter().map(|s| json!({ "id": s.0, "label": s.1, "labelNl": s.2, "color": s.3, "defaultMaterial": default_material(s.0) })).collect::<Vec<_>>(),
        "materials": MATERIALS.iter().map(|m| json!({ "id": m.id, "label": m.label, "labelNl": m.label_nl, "dns": m.dns, "dnLabels": m.dns.iter().map(|d| dn_label(m, *d)).collect::<Vec<_>>(), "joints": m.joints, "systems": m.systems, "bendUpTo": m.bend_up_to, "minBendRadiusFactor": m.min_bend, "bracketSpacing": m.spacing.iter().map(|(up, s)| json!({ "dnUpTo": up, "horizontal": s, "vertical": s * 1.6 })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "joints": JOINTS.iter().map(|j| json!({ "id": j.0, "label": j.1, "labelNl": j.2 })).collect::<Vec<_>>(),
        "fittings": FITTINGS.iter().map(|f| json!({ "id": f.id, "label": f.label, "labelNl": f.label_nl, "shape": format!("{:?}", f.shape), "systems": f.systems, "hasDn2": f.has_dn2 })).collect::<Vec<_>>(),
        "brackets": BRACKET_KINDS.iter().map(|b| json!({ "id": b.0, "label": b.1, "labelNl": b.2 })).collect::<Vec<_>>(),
        "insulation_mm": [0, 9, 13, 20, 25],
        "notes": ["dn = outer diameter in mm", "heights in pipe/fitting/bracket data are above the storey floor", "a bend is used at a corner when the material allows it for that dn (Alupex ≤ 26, PEX), otherwise an elbow of the pipe's joint type", "drains fall ≥ 1 % (checked)"]
    })
}
