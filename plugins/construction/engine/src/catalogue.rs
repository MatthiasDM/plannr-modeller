//! The component scripts that ship with the plugin (Studio's built-in types, verbatim) and the
//! prelude they run against. A document's own `componentTypes` add to or override these.

use crate::data::ComponentType;

/// Studio's component prelude: `component()`, `box()`, `shape()` and the `__` helpers.
pub const PRELUDE: &str = include_str!("../../catalogue/_prelude.js");

const BUILTIN: &[(&str, &str)] = &[
    ("base-unit", include_str!("../../catalogue/base-unit.js")),
    ("wall-unit", include_str!("../../catalogue/wall-unit.js")),
    ("tall-unit", include_str!("../../catalogue/tall-unit.js")),
    ("extractor", include_str!("../../catalogue/extractor.js")),
    ("worktop", include_str!("../../catalogue/worktop.js")),
    ("shelving", include_str!("../../catalogue/shelving.js")),
    ("radiator", include_str!("../../catalogue/radiator.js")),
    ("wardrobe", include_str!("../../catalogue/wardrobe.js")),
    ("island", include_str!("../../catalogue/island.js")),
    ("double-vanity", include_str!("../../catalogue/double-vanity.js")),
    ("bath", include_str!("../../catalogue/bath.js")),
    ("shower", include_str!("../../catalogue/shower.js")),
    ("wc", include_str!("../../catalogue/wc.js")),
    ("basin", include_str!("../../catalogue/basin.js")),
];

/// `// @key value` headers in the first lines of a script.
fn header(key: &str, source: &str) -> Option<String> {
    let tag = format!("// @{key} ");
    source.lines().take(8).find_map(|l| l.trim().strip_prefix(&tag).map(|v| v.trim().to_string()))
}

/// The built-in types, in catalogue order.
pub fn builtin() -> Vec<ComponentType> {
    BUILTIN.iter().map(|(id, src)| ComponentType {
        id: id.to_string(),
        name: header("name", src).unwrap_or_else(|| id.to_string()),
        group: header("group", src).unwrap_or_else(|| "Components".into()),
        source: src.to_string(),
    }).collect()
}
