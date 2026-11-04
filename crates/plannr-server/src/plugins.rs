//! Rust plugins compiled into the server: none. Plannr's plugins are installable packages (WebAssembly
//! engines, see `wasm.rs` and `packages.rs`) published in https://github.com/MatthiasDM/plannr-plugins.
//!
//! A server can still compile one in: implement `plannr_core::Plugin` in a crate, add it as a dependency
//! of this crate and register it here (`reg.register(Box::new(my_plugin::MyPlugin))`). Its ops become
//! `custom` ops and MCP tools `plugin.<id>.<op>`, and a browser bundle in `web/dist/plugins/<id>.js` is
//! loaded by the app. A package can't take the id of a compiled-in plugin.

use plannr_core::Registry;

pub fn registry() -> Registry { Registry::new() }
