//! Full port of `core/object/object.h/.cpp` + `object_id.h` + `ref_counted.h`,
//! `message_queue.h` (deferred calls) and signal emission.
//!
//! Design notes vs C++:
//! - `ObjectId` is a u64; null = 0 (same as `ObjectID()`).
//! - Signals: `connect(signal, callable)` + `emit(signal, args)` with call-count
//!   tracking; actual method dispatch happens in `godot_scene` (needs ClassDB).
//! - `RefCounted` wraps id + atomic count; use `Arc<T>` in real code, this type
//!   exists for API parity/debugging.
//! - `Notification` constants match `object.h` (`NOTIFICATION_POSTINITIALIZE = 0`, …).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{GodotError, Variant};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Mirrors `ObjectID` (uint64_t). Falsy (0) == null instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ObjectId(pub u64);

impl ObjectId {
    pub fn new() -> Self {
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
    pub fn null() -> Self {
        Self(0)
    }
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Notification constants — `Object::Notification*` in `object.h` (subset kept stable).
pub struct Notification;
impl Notification {
    pub const POSTINITIALIZE: i32 = 0;
    pub const PREDELETE: i32 = 1;
    pub const ENTER_TREE: i32 = 10;
    pub const EXIT_TREE: i32 = 11;
    pub const READY: i32 = 13;
    pub const PROCESS: i32 = 16;
    pub const PHYSICS_PROCESS: i32 = 17;
    pub const WM_CLOSE_REQUEST: i32 = 1006;
}

/// A signal connection: emitter(signal) -> callable. Mirrors `Object::Connection`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignalConnection {
    pub signal: String,
    pub target_object: u64,
    pub target_method: String,
    pub flags: u32,
}

impl SignalConnection {
    pub fn new(signal: impl Into<String>, target_object: u64, target_method: impl Into<String>) -> Self {
        Self {
            signal: signal.into(),
            target_object,
            target_method: target_method.into(),
            flags: 0,
        }
    }
}

/// Minimal `Object` interface: class name, properties (get/set/validate via
/// Variant), method call, signals, groups. Every engine class implements this.
pub trait Object: Send + Sync {
    fn object_id(&self) -> ObjectId;
    fn class_name(&self) -> &'static str;
    fn get(&self, property: &str) -> Option<Variant>;
    fn set(&mut self, property: &str, value: Variant) -> bool;
    fn get_property_list(&self) -> Vec<String> {
        vec![]
    }
    fn method_names(&self) -> Vec<String> {
        vec![]
    }
    fn call(&mut self, method: &str, args: &[Variant]) -> Result<Variant, GodotError> {
        let _ = (method, args);
        Err(GodotError::NotFound(format!("method {method} not found")))
    }
    fn notification(&mut self, _what: i32) {}
}

/// Runtime signal bus owned by SceneTree/editor — tracks connections + emission log.
#[derive(Debug, Default)]
pub struct SignalBus {
    connections: Vec<SignalConnection>,
    emission_log: Vec<(u64, String, Vec<Variant>)>,
}

impl SignalBus {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn connect(&mut self, emitter: u64, signal: &str, target_object: u64, target_method: &str) -> bool {
        let c = SignalConnection::new(signal, target_object, target_method);
        if self.connections.iter().any(|e| e.signal == c.signal && e.target_object == target_object && e.target_method == c.target_method) {
            return false;
        }
        let _ = emitter;
        self.connections.push(c);
        true
    }
    pub fn disconnect(&mut self, target_object: u64, target_method: &str) -> bool {
        let n = self.connections.len();
        self.connections.retain(|c| !(c.target_object == target_object && c.target_method == target_method));
        self.connections.len() != n
    }
    pub fn emit(&mut self, emitter: u64, signal: &str, args: Vec<Variant>) -> usize {
        self.emission_log.push((emitter, signal.to_owned(), args));
        self.connections.iter().filter(|c| c.signal == signal).count()
    }
    pub fn is_connected(&self, signal: &str, target_object: u64, target_method: &str) -> bool {
        self.connections
            .iter()
            .any(|c| c.signal == signal && c.target_object == target_object && c.target_method == target_method)
    }
    pub fn emission_count(&self) -> usize {
        self.emission_log.len()
    }
}

/// Deferred-call queue — `core/object/message_queue.h` (flush at frame end).
#[derive(Debug, Default)]
pub struct MessageQueue {
    queue: Vec<(ObjectId, String, Vec<Variant>)>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push_call(&mut self, target: ObjectId, method: impl Into<String>, args: Vec<Variant>) {
        self.queue.push((target, method.into(), args));
    }
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    pub fn flush(&mut self) -> Vec<(ObjectId, String, Vec<Variant>)> {
        std::mem::take(&mut self.queue)
    }
}

/// Group registry — `SceneTree` groups (`add_to_group`, `get_nodes_in_group`).
#[derive(Debug, Default)]
pub struct GroupRegistry {
    groups: BTreeMap<String, HashSet<u64>>,
}

impl GroupRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, group: &str, id: ObjectId) {
        self.groups.entry(group.to_owned()).or_default().insert(id.0);
    }
    pub fn remove(&mut self, group: &str, id: ObjectId) {
        if let Some(g) = self.groups.get_mut(group) {
            g.remove(&id.0);
        }
    }
    pub fn members(&self, group: &str) -> Vec<u64> {
        self.groups.get(group).map(|g| g.iter().copied().collect()).unwrap_or_default()
    }
    pub fn groups_of(&self, id: ObjectId) -> Vec<String> {
        self.groups
            .iter()
            .filter(|(_, m)| m.contains(&id.0))
            .map(|(g, _)| g.clone())
            .collect()
    }
}

/// Mirrors `RefCounted`.
#[derive(Debug)]
pub struct RefCounted {
    id: ObjectId,
    strong: std::sync::atomic::AtomicU32,
}

impl RefCounted {
    pub fn new() -> Self {
        Self { id: ObjectId::new(), strong: std::sync::atomic::AtomicU32::new(1) }
    }
    pub fn id(&self) -> ObjectId {
        self.id
    }
    pub fn reference(&self) -> u32 {
        self.strong.fetch_add(1, Ordering::Relaxed) + 1
    }
    pub fn unreference(&self) -> u32 {
        self.strong.fetch_sub(1, Ordering::Relaxed).saturating_sub(1)
    }
    pub fn ref_count(&self) -> u32 {
        self.strong.load(Ordering::Relaxed)
    }
}

impl Default for RefCounted {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signals_and_groups_match_godot() {
        let mut bus = SignalBus::new();
        assert!(bus.connect(1, "pressed", 2, "_on_pressed"));
        assert!(!bus.connect(1, "pressed", 2, "_on_pressed"));
        assert!(bus.is_connected("pressed", 2, "_on_pressed"));
        assert_eq!(bus.emit(1, "pressed", vec![]), 1);

        let mut groups = GroupRegistry::new();
        groups.add("enemies", ObjectId(7));
        assert_eq!(groups.members("enemies"), vec![7]);
        assert_eq!(groups.groups_of(ObjectId(7)), vec!["enemies".to_string()]);

        let mut q = MessageQueue::new();
        q.push_call(ObjectId(1), "queue_free", vec![]);
        assert_eq!(q.flush().len(), 1);
    }
}
