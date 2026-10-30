//! Port of `servers/rendering/rid_owner.h` (`RID_Alloc`) — typed free-list ids.
//! Godot RIDs are globally unique u64s minted per owner; here each `RidOwner`
//! mints locally-unique ids namespaced by a tag so cross-server collisions are
//! impossible and tests stay deterministic.

use godot_core::Rid;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Namespace tag so RIDs from different owners never collide meaningfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerTag(pub u64);

/// Typed RID storage with free-list reuse (mirrors `RID_Owner<T>`).
#[derive(Debug, Serialize, Deserialize)]
pub struct RidOwner<T> {
    tag: OwnerTag,
    next: u64,
    free: Vec<u64>,
    items: BTreeMap<u64, T>,
}

impl<T> RidOwner<T> {
    pub fn new() -> Self {
        Self {
            tag: OwnerTag(NEXT_OWNER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
            next: 1,
            free: vec![],
            items: BTreeMap::new(),
        }
    }
    pub fn alloc(&mut self, value: T) -> Rid {
        let local = self.free.pop().unwrap_or_else(|| {
            let n = self.next;
            self.next += 1;
            n
        });
        self.items.insert(local, value);
        // Encode owner tag in the high 32 bits (debug aid; `get_id` keeps local part).
        Rid((self.tag.0 << 32) | (local & 0xffff_ffff))
    }
    fn local(&self, rid: Rid) -> Option<u64> {
        let local = rid.0 & 0xffff_ffff;
        self.items.contains_key(&local).then_some(local)
    }
    pub fn get(&self, rid: Rid) -> Option<&T> {
        self.local(rid).and_then(|l| self.items.get(&l))
    }
    pub fn get_mut(&mut self, rid: Rid) -> Option<&mut T> {
        let local = rid.0 & 0xffff_ffff;
        self.items.get_mut(&local)
    }
    pub fn free(&mut self, rid: Rid) -> bool {
        let local = rid.0 & 0xffff_ffff;
        if self.items.remove(&local).is_some() {
            self.free.push(local);
            true
        } else {
            false
        }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn is_valid(&self, rid: Rid) -> bool {
        self.local(rid).is_some()
    }
    pub fn rids(&self) -> Vec<Rid> {
        self.items.keys().map(|l| Rid((self.tag.0 << 32) | l)).collect()
    }
}

impl<T> Default for RidOwner<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Untyped id allocator (timers, players, spaces) — same free-list idea.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RidAllocator {
    next: u64,
    free: Vec<u64>,
    live: usize,
}

impl RidAllocator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn alloc(&mut self) -> u64 {
        self.live += 1;
        self.free.pop().unwrap_or_else(|| {
            let n = self.next + 1;
            self.next = n;
            n
        })
    }
    pub fn free(&mut self, id: u64) -> bool {
        if id == 0 || id > self.next {
            return false;
        }
        self.free.push(id);
        self.live = self.live.saturating_sub(1);
        true
    }
    pub fn live(&self) -> usize {
        self.live
    }
}
