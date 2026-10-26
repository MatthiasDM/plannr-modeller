//! Undo/redo as document snapshots.

use crate::model::Document;

pub const MAX_HISTORY: usize = 200;

#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<(Document, String)>,
    redo: Vec<(Document, String)>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub version: u64,
    pub label: String,
}

impl History {
    /// Record the state *before* a change labelled `label`.
    pub fn push(&mut self, before: Document, label: impl Into<String>) {
        self.undo.push((before, label.into()));
        if self.undo.len() > MAX_HISTORY { self.undo.remove(0); }
        self.redo.clear();
    }
    /// Swap the current document for the previous snapshot; returns the undone label.
    pub fn undo(&mut self, current: &mut Document) -> Option<String> {
        let (doc, label) = self.undo.pop()?;
        let next_version = current.version + 1;
        self.redo.push((std::mem::replace(current, doc), label.clone()));
        current.version = next_version;
        Some(label)
    }
    pub fn redo(&mut self, current: &mut Document) -> Option<String> {
        let (doc, label) = self.redo.pop()?;
        let next_version = current.version + 1;
        self.undo.push((std::mem::replace(current, doc), label.clone()));
        current.version = next_version;
        Some(label)
    }
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.undo.iter().map(|(d, l)| HistoryEntry { version: d.version, label: l.clone() }).collect()
    }
}
