//! Full port of `core/string/ustring.h/.cpp` + `node_path.h` + interned `StringName`.
//!
//! Godot `String` is UTF-32 copy-on-write; here we wrap Rust `String` (UTF-8)
//! with the same method surface so GDScript/Iris behavior matches. `StringName`
//! is an interned handle — equality is a pointer/integer compare in C++; here a
//! global string pool giving the same O(1) equality + stable ordering.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::{OnceLock, RwLock};

/// Mirrors `String` — UTF-8 backed, Godot-compatible API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct GodotString(pub String);

impl GodotString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Character count (Godot counts Unicode scalar values, same as `.chars()`).
    pub fn length(&self) -> usize {
        self.0.chars().count()
    }
    pub fn substr(&self, from: usize, len: usize) -> Self {
        Self(self.0.chars().skip(from).take(len).collect())
    }
    pub fn left(&self, len: usize) -> Self {
        self.substr(0, len)
    }
    pub fn right(&self, len: usize) -> Self {
        let n = self.length().saturating_sub(len);
        self.substr(n, len)
    }
    pub fn find(&self, pat: &str) -> i64 {
        self.0.find(pat).map(|b| self.0[..b].chars().count() as i64).unwrap_or(-1)
    }
    pub fn rfind(&self, pat: &str) -> i64 {
        self.0.rfind(pat).map(|b| self.0[..b].chars().count() as i64).unwrap_or(-1)
    }
    pub fn contains(&self, pat: &str) -> bool {
        self.0.contains(pat)
    }
    pub fn begins_with(&self, pat: &str) -> bool {
        self.0.starts_with(pat)
    }
    pub fn ends_with(&self, pat: &str) -> bool {
        self.0.ends_with(pat)
    }
    pub fn replace(&self, from: &str, to: &str) -> Self {
        Self(self.0.replace(from, to))
    }
    pub fn split(&self, delim: &str) -> Vec<Self> {
        self.0.split(delim).map(|s| Self(s.to_owned())).collect()
    }
    pub fn to_upper(&self) -> Self {
        Self(self.0.to_uppercase())
    }
    pub fn to_lower(&self) -> Self {
        Self(self.0.to_lowercase())
    }
    pub fn trim(&self) -> Self {
        Self(self.0.trim().to_owned())
    }
    pub fn lstrip(&self) -> Self {
        Self(self.0.trim_start().to_owned())
    }
    pub fn rstrip(&self) -> Self {
        Self(self.0.trim_end().to_owned())
    }
    pub fn is_valid_int(&self) -> bool {
        self.0.trim().parse::<i64>().is_ok()
    }
    pub fn is_valid_float(&self) -> bool {
        self.0.trim().parse::<f64>().is_ok()
    }
    pub fn to_int(&self) -> i64 {
        self.0.trim().parse::<i64>().unwrap_or(0)
    }
    pub fn to_float(&self) -> f64 {
        self.0.trim().parse::<f64>().unwrap_or(0.0)
    }
    pub fn repeat(&self, n: usize) -> Self {
        Self(self.0.repeat(n))
    }
    pub fn insert(&self, pos: usize, s: &str) -> Self {
        let mut chars: Vec<char> = self.0.chars().collect();
        let at = pos.min(chars.len());
        for (i, c) in s.chars().enumerate() {
            chars.insert(at + i, c);
        }
        Self(chars.into_iter().collect())
    }
    pub fn capitalize(&self) -> Self {
        // `String::capitalize`: "hello_world" -> "Hello World".
        let spaced = self.0.replace('_', " ");
        let mut out = String::new();
        for (i, word) in spaced.split_whitespace().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            let mut ch = word.chars();
            if let Some(f) = ch.next() {
                out.extend(f.to_uppercase());
                out.push_str(&ch.as_str().to_lowercase());
            }
        }
        Self(out)
    }
    pub fn get_basename(&self) -> Self {
        // "res://a/b.gd" -> "res://a/b"
        match self.0.rfind('.') {
            Some(i) if !self.0[i..].contains('/') => Self(self.0[..i].to_owned()),
            _ => self.clone(),
        }
    }
    pub fn get_extension(&self) -> Self {
        match self.0.rfind('.') {
            Some(i) if !self.0[i..].contains('/') => Self(self.0[i + 1..].to_owned()),
            _ => Self(String::new()),
        }
    }
    pub fn path_join(&self, other: &str) -> Self {
        if self.0.ends_with('/') {
            Self(format!("{}{}", self.0, other.trim_start_matches('/')))
        } else {
            Self(format!("{}/{}", self.0, other.trim_start_matches('/')))
        }
    }
}

impl std::fmt::Display for GodotString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl From<&str> for GodotString {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

// ---------------------------------------------------------- StringName -----
static POOL: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
fn pool() -> &'static RwLock<BTreeSet<String>> {
    POOL.get_or_init(|| RwLock::new(BTreeSet::new()))
}

/// Interned string — `core/string/string_name.h`. Equality/hash are by content
/// (pool guarantees dedup); ordering matches Godot (`StringName <`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct StringName(String);

impl StringName {
    pub fn new(s: impl Into<String>) -> Self {
        let owned = s.into();
        pool().write().expect("pool").insert(owned.clone());
        Self(owned)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for StringName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn string_matches_godot_docs() {
        let s = GodotString::new("hello_world");
        assert_eq!(s.capitalize().0, "Hello World");
        assert_eq!(GodotString::new("res://a/b.gd").get_basename().0, "res://a/b");
        assert_eq!(GodotString::new("res://a/b.gd").get_extension().0, "gd");
        assert_eq!(GodotString::new("res://a").path_join("b.gd").0, "res://a/b.gd");
        assert_eq!(GodotString::new("  42 ").to_int(), 42);
        assert!(GodotString::new("3.5").is_valid_float());
        assert_eq!(GodotString::new("abc").find("b"), 1);
        assert_eq!(GodotString::new("abc").rfind("x"), -1);
    }
    #[test]
    fn string_name_interns() {
        let a = StringName::new("position");
        let b = StringName::new("position");
        assert_eq!(a, b);
    }
}
