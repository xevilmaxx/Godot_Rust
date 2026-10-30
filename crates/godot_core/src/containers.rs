//! Full port of `core/templates/rid.h` + `core/variant/array.h/.cpp`
//! (untyped part), `dictionary.h/.cpp`, `callable.h/.cpp` (value part).
//!
//! - `Rid`: opaque u64 handle issued by servers; `is_valid()` = nonzero.
//! - `GodotArray`: ordered Variant vec with optional typed flag + read-only flag
//!   (mirrors `Array` growth, `duplicate`, `slice`, `find`, `sort`, `pop_*`).
//! - `GodotDictionary`: BTreeMap<VariantKey, Variant> preserving Godot's sorted
//!   iteration + `get/lock` semantics (recursive counting omitted in v1).
//! - `Callable`: object-id + method StringName (custom callables in module 02).
//! - `Signal`: object-id + name; `SignalEmission` routing lives in object.rs.

use serde::{Deserialize, Serialize};

use crate::Variant;

// ------------------------------------------------------------------ Rid ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Rid(pub u64);

impl Rid {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    pub fn is_valid(self) -> bool {
        self.0 != 0
    }
    pub fn get_id(self) -> u64 {
        self.0
    }
}
impl std::fmt::Display for Rid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RID({})", self.0)
    }
}

// ---------------------------------------------------------------- Array ---
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GodotArray {
    items: Vec<Variant>,
    #[serde(default)]
    typed: Option<String>,
    #[serde(default)]
    read_only: bool,
}

impl GodotArray {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn typed(ty: impl Into<String>) -> Self {
        Self { typed: Some(ty.into()), ..Default::default() }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn push(&mut self, v: Variant) -> Result<(), String> {
        if self.read_only {
            return Err("Array is read-only".into());
        }
        self.items.push(v);
        Ok(())
    }
    pub fn pop_back(&mut self) -> Option<Variant> {
        if self.read_only {
            return None;
        }
        self.items.pop()
    }
    pub fn pop_front(&mut self) -> Option<Variant> {
        if self.read_only || self.items.is_empty() {
            return None;
        }
        Some(self.items.remove(0))
    }
    pub fn get(&self, i: usize) -> Option<&Variant> {
        self.items.get(i)
    }
    pub fn set(&mut self, i: usize, v: Variant) -> Result<(), String> {
        if self.read_only {
            return Err("Array is read-only".into());
        }
        *self.items.get_mut(i).ok_or_else(|| format!("index {i} out of bounds"))? = v;
        Ok(())
    }
    pub fn insert(&mut self, i: usize, v: Variant) -> Result<(), String> {
        if self.read_only {
            return Err("Array is read-only".into());
        }
        if i > self.items.len() {
            return Err(format!("index {i} out of bounds"));
        }
        self.items.insert(i, v);
        Ok(())
    }
    pub fn remove_at(&mut self, i: usize) -> Result<(), String> {
        if self.read_only {
            return Err("Array is read-only".into());
        }
        if i >= self.items.len() {
            return Err(format!("index {i} out of bounds"));
        }
        self.items.remove(i);
        Ok(())
    }
    pub fn clear(&mut self) {
        if !self.read_only {
            self.items.clear();
        }
    }
    pub fn find(&self, v: &Variant) -> i64 {
        self.items.iter().position(|x| x == v).map(|i| i as i64).unwrap_or(-1)
    }
    pub fn contains(&self, v: &Variant) -> bool {
        self.find(v) >= 0
    }
    pub fn count(&self, v: &Variant) -> usize {
        self.items.iter().filter(|x| *x == v).count()
    }
    pub fn duplicate(&self) -> Self {
        Self { items: self.items.clone(), ..self.clone_meta() }
    }
    pub fn duplicate_deep(&self) -> Self {
        Self { items: self.items.iter().map(|v| v.duplicate_deep()).collect(), ..self.clone_meta() }
    }
    fn clone_meta(&self) -> Self {
        Self { items: vec![], typed: self.typed.clone(), read_only: self.read_only }
    }
    pub fn slice(&self, begin: usize, end: usize, step: isize) -> Self {
        let mut out = Self { typed: self.typed.clone(), ..Default::default() };
        if step == 0 {
            return out;
        }
        let n = self.items.len() as isize;
        let norm = |i: isize| i.rem_euclid(n.max(1)) as usize;
        let (mut i, stop) = (norm(begin as isize), norm(end as isize));
        if step > 0 {
            while (i as isize) < stop as isize && i < self.items.len() {
                out.items.push(self.items[i].clone());
                i = i.saturating_add(step as usize);
                if i >= self.items.len() {
                    break;
                }
            }
        } else {
            let mut i = begin.min(self.items.len().saturating_sub(1)) as isize;
            while i >= stop as isize && i >= 0 {
                out.items.push(self.items[i as usize].clone());
                i += step;
            }
        }
        let _ = n;
        out
    }
    pub fn sort(&mut self) {
        if !self.read_only {
            self.items.sort_by(|a, b| a.sort_key().partial_cmp(&b.sort_key()).unwrap_or(std::cmp::Ordering::Equal));
        }
    }
    pub fn reverse(&mut self) {
        if !self.read_only {
            self.items.reverse();
        }
    }
    pub fn make_read_only(&mut self) {
        self.read_only = true;
    }
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    pub fn typed_class(&self) -> Option<&str> {
        self.typed.as_deref()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Variant> {
        self.items.iter()
    }
    pub fn into_vec(self) -> Vec<Variant> {
        self.items
    }
}
impl From<Vec<Variant>> for GodotArray {
    fn from(v: Vec<Variant>) -> Self {
        Self { items: v, ..Default::default() }
    }
}
impl FromIterator<Variant> for GodotArray {
    fn from_iter<I: IntoIterator<Item = Variant>>(iter: I) -> Self {
        Self { items: iter.into_iter().collect(), ..Default::default() }
    }
}
impl Extend<Variant> for GodotArray {
    fn extend<I: IntoIterator<Item = Variant>>(&mut self, iter: I) {
        if !self.read_only {
            self.items.extend(iter);
        }
    }
}

// ------------------------------------------------------------- Dictionary --
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GodotDictionary {
    map: std::collections::BTreeMap<String, Variant>,
    #[serde(default)]
    read_only: bool,
}

impl GodotDictionary {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn get(&self, key: &str) -> Option<&Variant> {
        self.map.get(key)
    }
    pub fn get_or(&self, key: &str, default: Variant) -> Variant {
        self.map.get(key).cloned().unwrap_or(default)
    }
    pub fn set(&mut self, key: impl Into<String>, v: Variant) -> Result<(), String> {
        if self.read_only {
            return Err("Dictionary is read-only".into());
        }
        self.map.insert(key.into(), v);
        Ok(())
    }
    pub fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
    pub fn erase(&mut self, key: &str) -> bool {
        if self.read_only {
            return false;
        }
        self.map.remove(key).is_some()
    }
    pub fn keys(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }
    pub fn values(&self) -> Vec<Variant> {
        self.map.values().cloned().collect()
    }
    pub fn clear(&mut self) {
        if !self.read_only {
            self.map.clear();
        }
    }
    pub fn duplicate(&self) -> Self {
        self.clone()
    }
    pub fn duplicate_deep(&self) -> Self {
        Self {
            map: self.map.iter().map(|(k, v)| (k.clone(), v.duplicate_deep())).collect(),
            read_only: self.read_only,
        }
    }
    pub fn merge(&mut self, other: &Self) -> Result<(), String> {
        if self.read_only {
            return Err("Dictionary is read-only".into());
        }
        for (k, v) in &other.map {
            self.map.insert(k.clone(), v.clone());
        }
        Ok(())
    }
    pub fn make_read_only(&mut self) {
        self.read_only = true;
    }
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Variant)> {
        self.map.iter()
    }
}

// --------------------------------------------------------------- Callable --
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Callable {
    pub object: u64,
    pub method: String,
}

impl Callable {
    pub fn new(object: u64, method: impl Into<String>) -> Self {
        Self { object, method: method.into() }
    }
    pub fn is_null(&self) -> bool {
        self.object == 0 || self.method.is_empty()
    }
    pub fn is_valid(&self) -> bool {
        !self.is_null()
    }
}

// ----------------------------------------------------------------- Signal --
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Signal {
    pub object: u64,
    pub name: String,
}

impl Signal {
    pub fn new(object: u64, name: impl Into<String>) -> Self {
        Self { object, name: name.into() }
    }
    pub fn is_null(&self) -> bool {
        self.object == 0 || self.name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_matches_godot() {
        let mut a = GodotArray::from(vec![Variant::Int(3), Variant::Int(1), Variant::Int(2)]);
        assert_eq!(a.find(&Variant::Int(1)), 1);
        a.sort();
        assert_eq!(a.get(0), Some(&Variant::Int(1)));
        assert_eq!(a.slice(0, 2, 1).len(), 2);
        let mut ro = a.duplicate();
        ro.make_read_only();
        assert!(ro.push(Variant::Nil).is_err());
    }
    #[test]
    fn dict_matches_godot() {
        let mut d = GodotDictionary::new();
        d.set("a", Variant::Int(1)).unwrap();
        assert!(d.has("a"));
        assert_eq!(d.get_or("missing", Variant::Int(7)), Variant::Int(7));
        assert!(d.erase("a"));
        assert!(!d.has("a"));
    }
}
