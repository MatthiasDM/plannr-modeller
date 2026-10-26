//! Rust plugins compiled into the server.
//!
//! To add one: implement `plannr_core::Plugin` in its own crate (see `plugins/construction/engine`),
//! add it as a dependency of this crate, then register it here:
//!
//! ```ignore
//! reg.register(Box::new(my_plugin::MyPlugin::default()));
//! ```
//!
//! A plugin's custom entities (`kind: "custom", type: "<plugin>.<name>"`) are expanded into faces
//! at compile time, its ops are reachable as `custom` ops with `type: "<plugin>.<op>"` and as MCP
//! tools named `plugin.<plugin>.<op>`, and its browser bundle (`web/dist/plugins/<plugin>.js`) is
//! loaded by the app when present.

use plannr_core::Registry;

pub fn registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Box::new(plannr_plugin_construction::ConstructionPlugin));
    reg.register(Box::new(plannr_plugin_electrical::ElectricalPlugin));
    reg.register(Box::new(plannr_plugin_framing::FramingPlugin));
    reg
}
