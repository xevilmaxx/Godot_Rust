//! Port of `modules/multiplayer/multiplayer_api.h` + `scene/multiplayer/*`.
//! Authority model: peer 1 is the server (Godot convention). RPCs are queued
//! per peer and drained by `poll()`; replication snapshots carry Variant maps.

use godot_core::{GodotError, Variant};
use std::collections::{BTreeMap, VecDeque};

pub const SERVER_ID: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMode {
    Authority,
    AnyPeer,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct RpcCall {
    pub from: u32,
    pub method: String,
    pub args: Vec<Variant>,
}

type RpcHandler = Box<dyn Fn(u32, &[Variant]) -> Result<Variant, GodotError> + Send + Sync>;

pub struct MultiplayerApi {
    local_id: u32,
    peers: BTreeMap<u32, VecDeque<RpcCall>>,
    handlers: BTreeMap<String, (RpcMode, RpcHandler)>,
    next_id: u32,
}

impl MultiplayerApi {
    /// Standalone (offline) API — `local_id` 1, like `SceneTree` without peers.
    pub fn new() -> Self {
        let mut peers = BTreeMap::new();
        peers.insert(SERVER_ID, VecDeque::new());
        Self { local_id: SERVER_ID, peers, handlers: BTreeMap::new(), next_id: 2 }
    }
    /// Loopback server + one client (tests and single-machine play).
    pub fn with_loopback_peer() -> (Self, u32) {
        let mut api = Self::new();
        let id = api.next_id;
        api.next_id += 1;
        api.peers.insert(id, VecDeque::new());
        (api, id)
    }
    pub fn local_id(&self) -> u32 {
        self.local_id
    }
    pub fn peer_ids(&self) -> Vec<u32> {
        self.peers.keys().copied().collect()
    }
    pub fn set_local_id(&mut self, id: u32) {
        self.local_id = id;
    }
    pub fn register_rpc(
        &mut self,
        method: &str,
        mode: RpcMode,
        handler: impl Fn(u32, &[Variant]) -> Result<Variant, GodotError> + Send + Sync + 'static,
    ) {
        self.handlers.insert(method.into(), (mode, Box::new(handler)));
    }
    /// Godot `rpc(method, args…)` — broadcast to every peer's queue (including self).
    pub fn rpc(&mut self, method: &str, args: Vec<Variant>) -> Result<usize, GodotError> {
        if !self.handlers.contains_key(method) {
            return Err(GodotError::NotFound(format!("rpc {method}")));
        }
        let from = self.local_id;
        let mut n = 0;
        for queue in self.peers.values_mut() {
            queue.push_back(RpcCall { from, method: method.into(), args: args.clone() });
            n += 1;
        }
        Ok(n)
    }
    /// Godot `rpc_id(peer, …)` — targeted call.
    pub fn rpc_id(&mut self, peer: u32, method: &str, args: Vec<Variant>) -> Result<(), GodotError> {
        if !self.handlers.contains_key(method) {
            return Err(GodotError::NotFound(format!("rpc {method}")));
        }
        let queue = self.peers.get_mut(&peer).ok_or_else(|| GodotError::NotFound(format!("peer {peer}")))?;
        queue.push_back(RpcCall { from: self.local_id, method: method.into(), args });
        Ok(())
    }
    /// Drain one peer's queue, enforcing `RpcMode`, running handlers.
    pub fn poll(&mut self, peer: u32) -> Vec<Result<Variant, GodotError>> {
        let calls: Vec<RpcCall> = self.peers.get_mut(&peer).map(|q| q.drain(..).collect()).unwrap_or_default();
        let mut out = vec![];
        for call in calls {
            let result = match self.handlers.get(&call.method) {
                None => Err(GodotError::NotFound(format!("rpc {}", call.method))),
                Some((mode, handler)) => match mode {
                    RpcMode::Disabled => Err(GodotError::InvalidArgument(format!("rpc {} disabled", call.method))),
                    RpcMode::AnyPeer => handler(call.from, &call.args),
                    RpcMode::Authority => {
                        if call.from == SERVER_ID || peer == SERVER_ID {
                            handler(call.from, &call.args)
                        } else {
                            Err(GodotError::Unauthorized(format!("rpc {} needs authority", call.method)))
                        }
                    }
                },
            };
            out.push(result);
        }
        out
    }
    pub fn pending(&self, peer: u32) -> usize {
        self.peers.get(&peer).map(|q| q.len()).unwrap_or(0)
    }
    /// Replication snapshot: full property map for one object (delta in module 08b).
    pub fn snapshot(object: &BTreeMap<String, Variant>) -> BTreeMap<String, Variant> {
        object.clone()
    }
}

impl Default for MultiplayerApi {
    fn default() -> Self {
        Self::new()
    }
}

// GodotError needs an Unauthorized variant for RPC authority — reuse InvalidArgument otherwise.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn broadcast_and_targeted_rpcs() {
        let (mut api, client) = MultiplayerApi::with_loopback_peer();
        let log = Arc::new(Mutex::new(vec![]));
        let l2 = log.clone();
        api.register_rpc("ping", RpcMode::AnyPeer, move |from, args| {
            l2.lock().unwrap().push((from, args.to_vec()));
            Ok(Variant::String("pong".into()))
        });
        api.set_local_id(SERVER_ID);
        assert_eq!(api.rpc("ping", vec![Variant::Int(1)]).unwrap(), 2);
        assert_eq!(api.pending(SERVER_ID), 1);
        assert_eq!(api.pending(client), 1);
        let r = api.poll(client);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].as_ref().unwrap(), &Variant::String("pong".into()));
        assert_eq!(log.lock().unwrap().len(), 1);
        api.rpc_id(SERVER_ID, "ping", vec![]).unwrap();
        assert_eq!(api.pending(SERVER_ID), 2); // earlier broadcast + targeted
    }
    #[test]
    fn authority_is_enforced() {
        let (mut api, client) = MultiplayerApi::with_loopback_peer();
        api.register_rpc("admin", RpcMode::Authority, |_, _| Ok(Variant::Nil));
        api.set_local_id(client); // non-server sender
        api.rpc("admin", vec![]).unwrap(); // queued to all
        let r = api.poll(client); // client executing a client-sent call → denied
        assert!(r[0].is_err());
    }
    #[test]
    fn unknown_rpc_errors() {
        let mut api = MultiplayerApi::new();
        assert!(api.rpc("nope", vec![]).is_err());
    }
}
