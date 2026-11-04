//! Plugin packages: `.plannr-plugin` files (zip) with a `plugin.json` manifest, an optional WebAssembly
//! engine (`engine.wasm`) and an optional browser bundle (`web.js`). Installed into
//! `<PLANNR_PLUGINS>/<id>/` (default `data/plugins`), loaded at startup, and enabled or disabled — built-in
//! plugins too — with the state kept in `<PLANNR_PLUGINS>/state.json`.

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use plannr_core::plugin::Source;
use plannr_core::Registry;
use serde::{Deserialize, Serialize};

use crate::wasm::WasmPlugin;

pub const EXT: &str = "plannr-plugin";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)] pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub license: Option<String>,
    /// Oldest Plannr version it runs on ("0.2").
    #[serde(default, skip_serializing_if = "Option::is_none")] pub plannr: Option<String>,
    /// File names inside the package (default engine.wasm / web.js when present).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub web: Option<String>,
    /// Plugins that work well with it (shown in the plugin manager).
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub suggests: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile { #[serde(default)] disabled: Vec<String> }

pub struct Packages {
    pub dir: PathBuf,
    /// Installed packages by id.
    pub installed: Mutex<BTreeMap<String, Manifest>>,
    /// Disabled plugin ids (built-in, installed and browser-only alike).
    pub disabled: Mutex<HashSet<String>>,
}

fn valid_id(id: &str) -> bool { (2..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !id.starts_with('-') }

impl Packages {
    /// Read the state and load every installed package into the registry.
    pub fn open(dir: PathBuf, reg: &Registry) -> Packages {
        let _ = std::fs::create_dir_all(&dir);
        let state: StateFile = std::fs::read(dir.join("state.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let p = Packages { dir, installed: Mutex::new(BTreeMap::new()), disabled: Mutex::new(state.disabled.into_iter().collect()) };
        for id in p.disabled.lock().unwrap().iter() { reg.set_enabled(id, false); }
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&p.dir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).filter(|d| d.join("plugin.json").exists()).collect()).unwrap_or_default();
        dirs.sort();
        for d in dirs {
            match p.load_dir(&d, reg) {
                Ok(m) => tracing::info!("installed plugin {} {} ({})", m.id, m.version, d.display()),
                Err(e) => tracing::warn!("plugin in {} not loaded: {e}", d.display()),
            }
        }
        p
    }

    fn load_dir(&self, d: &Path, reg: &Registry) -> anyhow::Result<Manifest> {
        let m: Manifest = serde_json::from_slice(&std::fs::read(d.join("plugin.json"))?)?;
        if !valid_id(&m.id) { anyhow::bail!("invalid plugin id {:?}", m.id); }
        let engine = d.join(m.engine.as_deref().unwrap_or("engine.wasm"));
        if engine.exists() {
            let w = WasmPlugin::load(&std::fs::read(&engine)?)?;
            if plannr_core::Plugin::id(&w) != m.id { anyhow::bail!("engine says it is {:?}, the manifest {:?}", plannr_core::Plugin::id(&w), m.id); }
            let enabled = !self.disabled.lock().unwrap().contains(&m.id);
            reg.add(Arc::new(w), Source::Installed, enabled);
        }
        self.installed.lock().unwrap().insert(m.id.clone(), m.clone());
        Ok(m)
    }

    fn save(&self) {
        let mut disabled: Vec<String> = self.disabled.lock().unwrap().iter().cloned().collect(); disabled.sort();
        let _ = std::fs::write(self.dir.join("state.json"), serde_json::to_vec_pretty(&StateFile { disabled }).unwrap_or_default());
    }

    pub fn is_enabled(&self, id: &str) -> bool { !self.disabled.lock().unwrap().contains(id) }
    pub fn set_enabled(&self, id: &str, enabled: bool, reg: &Registry) {
        { let mut d = self.disabled.lock().unwrap(); if enabled { d.remove(id); } else { d.insert(id.to_string()); } }
        reg.set_enabled(id, enabled);
        self.save();
    }

    /// Path of an installed package's browser bundle.
    pub fn script(&self, id: &str) -> Option<PathBuf> {
        let m = self.installed.lock().unwrap().get(id).cloned()?;
        let f = self.dir.join(&m.id).join(m.web.as_deref().unwrap_or("web.js"));
        f.exists().then_some(f)
    }

    /// Install (or update) from a package's bytes. `builtin` ids can't be taken over.
    pub fn install(&self, bytes: &[u8], reg: &Registry, builtin: &HashSet<String>) -> anyhow::Result<Manifest> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| anyhow::anyhow!("not a plugin package (a .{EXT} is a zip file): {e}"))?;
        // the files may sit at the root or in one folder
        let names: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string())).collect();
        let manifest_path = names.iter().filter(|n| n.ends_with("plugin.json") && n.matches('/').count() <= 1).min_by_key(|n| n.len()).ok_or_else(|| anyhow::anyhow!("the package has no plugin.json"))?.clone();
        let prefix = manifest_path.trim_end_matches("plugin.json").to_string();
        let mut read = |name: &str, cap: u64| -> anyhow::Result<Option<Vec<u8>>> {
            let Ok(f) = zip.by_name(&format!("{prefix}{name}")) else { return Ok(None) };
            if f.size() > cap { anyhow::bail!("{name} is larger than {} MB", cap >> 20); }
            let mut b = vec![]; f.take(cap + 1).read_to_end(&mut b)?; Ok(Some(b))
        };
        let m: Manifest = serde_json::from_slice(&read("plugin.json", 1 << 20)?.unwrap()).map_err(|e| anyhow::anyhow!("plugin.json: {e}"))?;
        if !valid_id(&m.id) { anyhow::bail!("invalid plugin id {:?} (2–40 of a-z, 0-9, -)", m.id); }
        if builtin.contains(&m.id) { anyhow::bail!("{} is built into this server", m.id); }
        if let Some(need) = &m.plannr { if version_lt(plannr_core::VERSION, need) { anyhow::bail!("{} needs Plannr {need} or newer (this is {})", m.name, plannr_core::VERSION); } }
        let engine_name = m.engine.clone().unwrap_or_else(|| "engine.wasm".into());
        let web_name = m.web.clone().unwrap_or_else(|| "web.js".into());
        for n in [&engine_name, &web_name] { if n.contains('/') || n.contains('\\') || n.starts_with('.') { anyhow::bail!("bad file name {n:?} in plugin.json"); } }
        let engine = read(&engine_name, 64 << 20)?;
        let web = read(&web_name, 16 << 20)?;
        let readme = read("README.md", 1 << 20)?;
        let license = read("LICENSE", 1 << 20)?;
        if engine.is_none() && web.is_none() { anyhow::bail!("the package has neither {engine_name} nor {web_name}"); }
        // check the engine runs before anything is written
        if let Some(e) = &engine {
            let w = WasmPlugin::load(e).map_err(|x| anyhow::anyhow!("its engine does not load: {x}"))?;
            if plannr_core::Plugin::id(&w) != m.id { anyhow::bail!("its engine says it is {:?}, the manifest {:?}", plannr_core::Plugin::id(&w), m.id); }
        }
        let tmp = self.dir.join(format!(".{}.tmp", m.id));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("plugin.json"), serde_json::to_vec_pretty(&m)?)?;
        if let Some(b) = &engine { std::fs::write(tmp.join(&engine_name), b)?; }
        if let Some(b) = &web { std::fs::write(tmp.join(&web_name), b)?; }
        if let Some(b) = &readme { std::fs::write(tmp.join("README.md"), b)?; }
        if let Some(b) = &license { std::fs::write(tmp.join("LICENSE"), b)?; }
        let dest = self.dir.join(&m.id);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&tmp, &dest)?;
        reg.remove(&m.id);
        self.load_dir(&dest, reg)
    }

    pub fn uninstall(&self, id: &str, reg: &Registry) -> anyhow::Result<()> {
        if self.installed.lock().unwrap().remove(id).is_none() { anyhow::bail!("{id} is not an installed package"); }
        reg.remove(id);
        std::fs::remove_dir_all(self.dir.join(id))?;
        self.disabled.lock().unwrap().remove(id);
        self.save();
        Ok(())
    }
}

/// "0.2.0" < "0.3"?
fn version_lt(have: &str, need: &str) -> bool {
    let p = |s: &str| s.split('.').map(|x| x.trim().parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (p(have), p(need));
    for i in 0..a.len().max(b.len()) { let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)); if x != y { return x < y; } }
    false
}
