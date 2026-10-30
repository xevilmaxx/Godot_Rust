//! Full port of `core/string/node_path.h/.cpp`.
//! Format: `[absolute/relative] name1/name2/... [: subname1:subname2 ...]`
//! e.g. `/root/Player/Sprite2D:texture:filter`. Property subnames after `:`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodePath {
    pub raw: String,
    pub is_absolute: bool,
    pub names: Vec<String>,
    pub subnames: Vec<String>,
}

impl NodePath {
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let mut p = Self { raw: raw.clone(), is_absolute: false, names: vec![], subnames: vec![] };
        p.parse();
        p
    }
    fn parse(&mut self) {
        let s = self.raw.trim();
        self.is_absolute = s.starts_with('/');
        // Split off `:subname` tail — but only after the node portion.
        // Godot treats the FIRST `:` as the subname separator (node names never contain `:`).
        let (node_part, sub_part) = match s.split_once(':') {
            Some((n, s)) => (n, Some(s)),
            None => (s, None),
        };
        self.names = node_part
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .map(|p| p.to_owned())
            .collect();
        self.subnames = sub_part
            .map(|r| r.split(':').filter(|p| !p.is_empty()).map(|p| p.to_owned()).collect())
            .unwrap_or_default();
    }
    pub fn is_absolute(&self) -> bool {
        self.is_absolute
    }
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.subnames.is_empty()
    }
    pub fn get_name_count(&self) -> usize {
        self.names.len()
    }
    pub fn get_subname_count(&self) -> usize {
        self.subnames.len()
    }
    pub fn get_name(&self, i: usize) -> Option<&str> {
        self.names.get(i).map(|s| s.as_str())
    }
    pub fn get_subname(&self, i: usize) -> Option<&str> {
        self.subnames.get(i).map(|s| s.as_str())
    }
    /// Last node name — e.g. `Sprite2D` for `/root/Player/Sprite2D:texture`.
    pub fn last_name(&self) -> Option<&str> {
        self.names.last().map(|s| s.as_str())
    }
}

impl From<&str> for NodePath {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_godot_examples() {
        let p = NodePath::new("/root/Player/Sprite2D:texture:filter");
        assert!(p.is_absolute());
        assert_eq!(p.names, vec!["root", "Player", "Sprite2D"]);
        assert_eq!(p.subnames, vec!["texture", "filter"]);
        assert_eq!(p.last_name(), Some("Sprite2D"));
        let rel = NodePath::new("../Sibling/Node");
        assert!(!rel.is_absolute());
        assert_eq!(rel.get_name_count(), 3);
    }
}
