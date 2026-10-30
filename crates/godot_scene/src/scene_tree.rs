//! Port of `scene/main/scene_tree.h/.cpp` — registry, paths, groups, main loop.
//! Single-threaded (Godot's workers arrive with `godot_host`); deterministic
//! frame stepping so Iris agents and tests observe identical results.

use godot_core::{GodotError, GroupRegistry, MessageQueue, NodePath, Object, ObjectId, SignalBus, Variant};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{Node, Node2D, Node3D};

/// Heterogeneous storage — every engine node is one of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoredNode {
    Base(Node),
    N2D(Node2D),
    N3D(Node3D),
}

impl StoredNode {
    pub fn id(&self) -> ObjectId {
        match self {
            StoredNode::Base(n) => n.data.id,
            StoredNode::N2D(n) => n.base.data.id,
            StoredNode::N3D(n) => n.base.data.id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            StoredNode::Base(n) => &n.data.name,
            StoredNode::N2D(n) => &n.base.data.name,
            StoredNode::N3D(n) => &n.base.data.name,
        }
    }
    pub fn class(&self) -> &str {
        match self {
            StoredNode::Base(n) => &n.data.class,
            StoredNode::N2D(_) => "Node2D",
            StoredNode::N3D(_) => "Node3D",
        }
    }
    pub fn data_mut(&mut self) -> &mut crate::NodeData {
        match self {
            StoredNode::Base(n) => &mut n.data,
            StoredNode::N2D(n) => &mut n.base.data,
            StoredNode::N3D(n) => &mut n.base.data,
        }
    }
    pub fn data(&self) -> &crate::NodeData {
        match self {
            StoredNode::Base(n) => &n.data,
            StoredNode::N2D(n) => &n.base.data,
            StoredNode::N3D(n) => &n.base.data,
        }
    }
    pub fn get_prop(&self, prop: &str) -> Option<Variant> {
        match self {
            StoredNode::Base(n) => n.get(prop),
            StoredNode::N2D(n) => n.get(prop),
            StoredNode::N3D(n) => n.get(prop),
        }
    }
    pub fn set_prop(&mut self, prop: &str, v: Variant) -> bool {
        match self {
            StoredNode::Base(n) => n.set(prop, v),
            StoredNode::N2D(n) => n.set(prop, v),
            StoredNode::N3D(n) => n.set(prop, v),
        }
    }
}

/// `SceneTreeTimer` — `scene/main/timer_process.h` subset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeTimer {
    pub id: u64,
    pub wait_time: f64,
    pub time_left: f64,
    pub process_always: bool,
    pub ignore_time_scale: bool,
    pub expired: bool,
}

pub struct SceneTree {
    nodes: BTreeMap<u64, StoredNode>,
    root_id: u64,
    current_scene: Option<u64>,
    pub paused: bool,
    pub time_scale: f64,
    pub idle_frame: u64,
    pub physics_frame: u64,
    pub idle_time: f64,
    pub physics_time: f64,
    groups: GroupRegistry,
    signals: SignalBus,
    queue: MessageQueue,
    timers: BTreeMap<u64, TreeTimer>,
    next_timer: u64,
    /// Attached Iris sources (`node id -> source`). Executed by `process_frame`.
    pub scripts: BTreeMap<u64, String>,
    /// `_ready` already ran per node (Godot calls it once on tree entry).
    ready_run: BTreeSet<u64>,
    /// Captured `print` output per node (observable by tests, MCP, editor).
    script_output: BTreeMap<u64, Vec<String>>,
    pub frame: u64, // alias of idle_frame for CLI compat
}

impl SceneTree {
    pub fn new() -> Self {
        let root = Node::new("root");
        let id = root.data.id.0;
        let mut nodes = BTreeMap::new();
        let mut root = root;
        root.data.inside_tree = true;
        nodes.insert(id, StoredNode::Base(root));
        Self {
            nodes,
            root_id: id,
            current_scene: None,
            paused: false,
            time_scale: 1.0,
            idle_frame: 0,
            physics_frame: 0,
            idle_time: 0.0,
            physics_time: 0.0,
            groups: GroupRegistry::new(),
            signals: SignalBus::new(),
            queue: MessageQueue::new(),
            timers: BTreeMap::new(),
            next_timer: 1,
            scripts: BTreeMap::new(),
            ready_run: BTreeSet::new(),
            script_output: BTreeMap::new(),
            frame: 0,
        }
    }

    pub fn root_id(&self) -> ObjectId {
        ObjectId(self.root_id)
    }
    pub fn current_scene(&self) -> Option<ObjectId> {
        self.current_scene.map(ObjectId)
    }
    pub fn set_current_scene(&mut self, id: Option<ObjectId>) {
        self.current_scene = id.map(|o| o.0);
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn contains(&self, id: ObjectId) -> bool {
        self.nodes.contains_key(&id.0)
    }

    // ------------------------------------------------------------ add/remove
    fn sibling_names(&self, parent: u64) -> Vec<String> {
        self.nodes
            .get(&parent)
            .map(|p| {
                p.data()
                    .children
                    .iter()
                    .filter_map(|c| self.nodes.get(&c.0).map(|n| n.name().to_owned()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn insert(&mut self, mut node: StoredNode, parent: Option<ObjectId>, force_readable: bool) -> Result<ObjectId, GodotError> {
        let pid = parent.map(|p| p.0).unwrap_or(self.root_id);
        if !self.nodes.contains_key(&pid) {
            return Err(GodotError::NotFound(format!("parent {pid} not found")));
        }
        let wanted = node.name().to_owned();
        let unique = self
            .nodes
            .get(&pid)
            .expect("parent")
            .data()
            .validate_child_name(&self.sibling_names(pid), &wanted, force_readable)?;
        node.data_mut().name = unique;
        let id = node.id();
        node.data_mut().parent = Some(ObjectId(pid));
        node.data_mut().inside_tree = true;
        for g in node.data().groups.clone() {
            self.groups.add(&g, id);
        }
        self.nodes.get_mut(&pid).expect("parent").data_mut().children.push(id);
        self.nodes.insert(id.0, node);
        self.signals.emit(pid, "child_entered_tree", vec![Variant::Object(id.0)]);
        Ok(id)
    }

    pub fn add_base(&mut self, node: Node, parent: Option<ObjectId>) -> Result<ObjectId, GodotError> {
        self.insert(StoredNode::Base(node), parent, false)
    }
    pub fn add_2d(&mut self, node: Node2D, parent: Option<ObjectId>) -> Result<ObjectId, GodotError> {
        self.insert(StoredNode::N2D(node), parent, false)
    }
    pub fn add_3d(&mut self, node: Node3D, parent: Option<ObjectId>) -> Result<ObjectId, GodotError> {
        self.insert(StoredNode::N3D(node), parent, false)
    }
    /// Back-compat shim for the module-01 CLI (`add_node` never failed there).
    pub fn add_node(&mut self, node: Node, parent: Option<ObjectId>) -> ObjectId {
        self.add_base(node, parent).expect("add_node")
    }

    pub fn remove_child(&mut self, parent: ObjectId, child: ObjectId) -> Result<(), GodotError> {
        let p = self.nodes.get_mut(&parent.0).ok_or_else(|| GodotError::NotFound(format!("parent {}", parent.0)))?;
        p.data_mut().children.retain(|c| *c != child);
        if let Some(c) = self.nodes.get_mut(&child.0) {
            c.data_mut().parent = None;
            c.data_mut().inside_tree = false;
        }
        Ok(())
    }

    /// Delete now (cascades to descendants). `queue_free` defers to frame end.
    pub fn free(&mut self, id: ObjectId) -> Result<(), GodotError> {
        if id.0 == self.root_id {
            return Err(GodotError::InvalidArgument("cannot free root".into()));
        }
        let node = self.nodes.get(&id.0).ok_or_else(|| GodotError::NotFound(format!("node {}", id.0)))?.clone();
        for child in node.data().children.clone() {
            self.free(child)?;
        }
        if let Some(pid) = node.data().parent {
            if let Some(p) = self.nodes.get_mut(&pid.0) {
                p.data_mut().children.retain(|c| *c != id);
            }
        }
        for g in node.data().groups.iter() {
            self.groups.remove(g, id);
        }
        self.scripts.remove(&id.0);
        self.nodes.remove(&id.0);
        Ok(())
    }

    pub fn queue_free(&mut self, id: ObjectId) -> Result<(), GodotError> {
        self.nodes
            .get_mut(&id.0)
            .ok_or_else(|| GodotError::NotFound(format!("node {}", id.0)))?
            .data_mut()
            .queued_for_deletion = true;
        Ok(())
    }

    fn flush_queued(&mut self) {
        let queued: Vec<ObjectId> =
            self.nodes.values().filter(|n| n.data().queued_for_deletion).map(|n| n.id()).collect();
        for id in queued {
            // Already freed as part of an ancestor cascade — skip.
            if self.nodes.contains_key(&id.0) {
                let _ = self.free(id);
            }
        }
    }

    // ------------------------------------------------------------------ get
    pub fn get_stored(&self, id: ObjectId) -> Result<&StoredNode, GodotError> {
        self.nodes.get(&id.0).ok_or_else(|| GodotError::NotFound(format!("node {}", id.0)))
    }
    pub fn get_stored_mut(&mut self, id: ObjectId) -> Result<&mut StoredNode, GodotError> {
        self.nodes.get_mut(&id.0).ok_or_else(|| GodotError::NotFound(format!("node {}", id.0)))
    }
    /// Legacy accessor used by the module-01 editor (Base nodes only).
    pub fn get_node(&self, id: ObjectId) -> Result<&Node, GodotError> {
        match self.get_stored(id)? {
            StoredNode::Base(n) => Ok(n),
            _ => Err(GodotError::InvalidArgument(format!("node {} is not a base Node", id.0))),
        }
    }
    pub fn get_node_mut(&mut self, id: ObjectId) -> Result<&mut Node, GodotError> {
        match self.get_stored_mut(id)? {
            StoredNode::Base(n) => Ok(n),
            _ => Err(GodotError::InvalidArgument(format!("node {} is not a base Node", id.0))),
        }
    }

    pub fn get_child(&self, parent: ObjectId, idx: usize) -> Result<ObjectId, GodotError> {
        let p = self.get_stored(parent)?;
        p.data().children.get(idx).copied().ok_or_else(|| GodotError::Engine {
            code: godot_core::Error::OutOfBounds,
            message: format!("child index {idx}"),
        })
    }
    pub fn get_child_count(&self, parent: ObjectId) -> Result<usize, GodotError> {
        Ok(self.get_stored(parent)?.data().children.len())
    }
    pub fn get_children(&self, parent: ObjectId) -> Result<Vec<ObjectId>, GodotError> {
        Ok(self.get_stored(parent)?.data().children.clone())
    }

    /// `find_child(pattern, recursive, owned)` — `*`/`?` glob on node names.
    pub fn find_child(&self, from: ObjectId, pattern: &str, recursive: bool, owned: bool) -> Result<Option<ObjectId>, GodotError> {
        let kids = self.get_children(from)?;
        for kid in &kids {
            let n = self.get_stored(*kid)?;
            if glob_match(pattern, n.name()) && (!owned || n.data().owner.is_some()) {
                return Ok(Some(*kid));
            }
        }
        if recursive {
            for kid in &kids {
                if let Some(found) = self.find_child(*kid, pattern, true, owned)? {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }

    /// Resolve a `NodePath` from `from` (absolute paths start at root;
    /// `%Unique` searches unique-name nodes; `..`/`.` supported).
    pub fn get_node_by_path(&self, from: ObjectId, path: &NodePath) -> Result<ObjectId, GodotError> {
        if path.is_empty() {
            return Err(GodotError::InvalidArgument("empty path".into()));
        }
        // Property subnames (`Sprite:texture`) resolve to the node itself here;
        // callers use get_prop on the result (matches editor behavior).
        let mut cur = if path.is_absolute { ObjectId(self.root_id) } else { from };
        let root_name = self.nodes.get(&self.root_id).map(|n| n.name().to_owned()).unwrap_or_default();
        let mut names = path.names.iter().peekable();
        // `/root/...` — the first component names the root itself; skip it.
        if path.is_absolute {
            if let Some(first) = names.peek() {
                if first.as_str() == root_name {
                    names.next();
                }
            }
        }
        for name in names {
            if name == "." {
                continue;
            } else if name == ".." {
                let p = self.get_stored(cur)?.data().parent.ok_or_else(|| GodotError::NotFound("no parent".into()))?;
                cur = p;
            } else if let Some(unique) = name.strip_prefix('%') {
                cur = self.find_unique(unique)?.ok_or_else(|| GodotError::NotFound(format!("unique %{unique}")))?;
            } else {
                let kids = self.get_children(cur)?;
                let mut found = None;
                for kid in kids {
                    if self.get_stored(kid)?.name() == name {
                        found = Some(kid);
                        break;
                    }
                }
                cur = found.ok_or_else(|| GodotError::NotFound(format!("`{name}` not found under {}", cur.0)))?;
            }
        }
        Ok(cur)
    }

    fn find_unique(&self, name: &str) -> Result<Option<ObjectId>, GodotError> {
        for (id, n) in &self.nodes {
            if n.name() == name && n.data().unique_name_in_owner {
                return Ok(Some(ObjectId(*id)));
            }
        }
        Ok(None)
    }

    /// Absolute path of a node (`/root/A/B`), or relative `get_path_to`.
    pub fn get_path(&self, id: ObjectId) -> Result<NodePath, GodotError> {
        let mut parts = vec![];
        let mut cur = Some(id);
        while let Some(c) = cur {
            let n = self.get_stored(c)?;
            parts.push(n.name().to_owned());
            if c.0 == self.root_id {
                break;
            }
            cur = n.data().parent;
        }
        parts.reverse();
        Ok(NodePath::new(format!("/{}", parts.join("/"))))
    }

    pub fn get_prop(&self, id: ObjectId, prop: &str) -> Result<Variant, GodotError> {
        // `Sprite:texture`-style subnames: split and resolve the node part.
        let (node_part, sub) = match prop.split_once(':') {
            Some((n, s)) if !n.is_empty() => (Some(n), Some(s)),
            _ => (None, None),
        };
        let target = match node_part {
            Some(p) => self.get_node_by_path(id, &NodePath::new(p))?,
            None => id,
        };
        let key = sub.unwrap_or(prop);
        self.get_stored(target)?.get_prop(key).ok_or_else(|| GodotError::NotFound(format!("property {key}")))
    }

    pub fn set_prop(&mut self, id: ObjectId, prop: &str, value: Variant) -> Result<(), GodotError> {
        if !self.get_stored_mut(id)?.set_prop(prop, value) {
            return Err(GodotError::InvalidArgument(format!("cannot set {prop}")));
        }
        Ok(())
    }

    /// Pretty tree (`Node::print_tree_pretty`) — used by CLI + MCP.
    pub fn print_tree_pretty(&self) -> String {
        let mut out = String::new();
        self.print_sub(ObjectId(self.root_id), "", true, &mut out);
        out
    }
    fn print_sub(&self, id: ObjectId, prefix: &str, last: bool, out: &mut String) {
        if let Ok(n) = self.get_stored(id) {
            let branch = if id.0 == self.root_id { "●" } else if last { "└─" } else { "├─" };
            out.push_str(&format!("{prefix}{branch} {} [{}]\n", n.name(), n.class()));
            let kids = n.data().children.clone();
            let child_prefix = if id.0 == self.root_id {
                String::new()
            } else {
                format!("{prefix}{}", if last { "  " } else { "│ " })
            };
            for (i, kid) in kids.iter().enumerate() {
                let is_last = i + 1 == kids.len();
                self.print_sub(*kid, &child_prefix, is_last, out);
            }
        }
    }

    // ---------------------------------------------------------------- groups
    pub fn add_to_group(&mut self, id: ObjectId, group: &str) -> Result<(), GodotError> {
        self.get_stored_mut(id)?.data_mut().groups.insert(group.to_owned());
        self.groups.add(group, id);
        Ok(())
    }
    pub fn remove_from_group(&mut self, id: ObjectId, group: &str) -> Result<(), GodotError> {
        self.get_stored_mut(id)?.data_mut().groups.remove(group);
        self.groups.remove(group, id);
        Ok(())
    }
    pub fn get_nodes_in_group(&self, group: &str) -> Vec<ObjectId> {
        self.groups.members(group).into_iter().map(ObjectId).collect()
    }
    /// `SceneTree::call_group` — sets `property = value` on members in v1
    /// (method dispatch arrives with script instances in module 05).
    pub fn call_group_set(&mut self, group: &str, property: &str, value: Variant) -> usize {
        let members = self.get_nodes_in_group(group);
        let mut n = 0;
        for id in members {
            if let Ok(s) = self.get_stored_mut(id) {
                if s.set_prop(property, value.clone()) {
                    n += 1;
                }
            }
        }
        n
    }

    // ---------------------------------------------------------------- timers
    pub fn create_timer(&mut self, wait: f64, process_always: bool, ignore_time_scale: bool) -> u64 {
        let id = self.next_timer;
        self.next_timer += 1;
        self.timers.insert(
            id,
            TreeTimer { id, wait_time: wait, time_left: wait, process_always, ignore_time_scale, expired: false },
        );
        id
    }
    pub fn timer_time_left(&self, id: u64) -> Result<f64, GodotError> {
        self.timers.get(&id).map(|t| t.time_left).ok_or_else(|| GodotError::NotFound(format!("timer {id}")))
    }
    fn tick_timers(&mut self, delta: f64) -> Vec<u64> {
        let scaled = |t: &TreeTimer| if t.ignore_time_scale { delta } else { delta * self.time_scale };
        let mut expired = vec![];
        for t in self.timers.values_mut() {
            if t.expired {
                continue;
            }
            if self.paused && !t.process_always {
                continue;
            }
            t.time_left -= scaled(t);
            if t.time_left <= 0.0 {
                t.expired = true;
                expired.push(t.id);
            }
        }
        for id in &expired {
            self.signals.emit(0, &format!("timer_{id}_timeout"), vec![]);
        }
        expired
    }

    // ------------------------------------------------------------ main loop
    /// Idle frame (`Main::iteration`): timers, Iris `_process`, deferred calls,
    /// queued frees. Returns expired timer ids (timeout signal observers).
    pub fn process_frame(&mut self, delta: f64) -> Vec<u64> {
        let expired = self.tick_timers(delta);
        if !self.paused {
            self.idle_time += delta * self.time_scale;
        }
        self.run_scripts(delta, false);
        for (target, method, args) in self.queue.flush() {
            let _ = self.call_method(target, &method, &args);
        }
        self.flush_queued();
        self.idle_frame += 1;
        self.frame = self.idle_frame;
        expired
    }
    pub fn physics_frame(&mut self, delta: f64) {
        if !self.paused {
            self.physics_time += delta * self.time_scale;
        }
        self.run_scripts(delta, true);
        self.physics_frame += 1;
    }
    /// Execute attached Iris scripts: `_ready` once, then `_process` (idle)
    /// or `_physics_process` (physics) each frame. Honors `paused`.
    fn run_scripts(&mut self, delta: f64, physics: bool) {
        if self.paused {
            return;
        }
        let ids: Vec<u64> = self.scripts.keys().copied().collect();
        for id in ids {
            // Skip scripts on dead nodes (freed but re-added scripts map entry).
            if !self.nodes.contains_key(&id) {
                continue;
            }
            // Respect per-node processing flags (matches `Node::can_process`).
            let can = self
                .nodes
                .get(&id)
                .map(|n| {
                    let d = n.data();
                    if physics {
                        d.physics_processing
                    } else {
                        d.processing || !d.ready_notified
                    }
                })
                .unwrap_or(false);
            // `_ready` runs even when processing is off (first frame only).
            let src = match self.scripts.get(&id) {
                Some(s) => s.clone(),
                None => continue,
            };
            let file = match godot_script_iris::parse(&src) {
                Ok(f) => f,
                Err(_) => continue,
            };
            if !self.ready_run.contains(&id) {
                self.ready_run.insert(id);
                if let Some(func) = file.fns.iter().find(|f| f.name == "_ready") {
                    let mut interp = godot_script_iris::Interpreter::new();
                    interp.run(&func.body);
                    self.script_output.entry(id).or_default().extend(interp.output);
                }
                if let Some(n) = self.nodes.get_mut(&id) {
                    n.data_mut().ready_notified = true;
                }
            }
            if physics && !self.nodes.get(&id).map(|n| n.data().physics_processing).unwrap_or(false) {
                continue;
            }
            if !physics && !can {
                continue;
            }
            let want = if physics { "_physics_process" } else { "_process" };
            if let Some(func) = file.fns.iter().find(|f| f.name == want) {
                let mut interp = godot_script_iris::Interpreter::new();
                interp.vars.insert("dt".to_owned(), godot_script_iris::Value::Float(delta));
                // Preset node + script props so scripts observe and move the node.
                if let Some(n) = self.nodes.get(&id) {
                    interp
                        .vars
                        .insert("name".to_owned(), godot_script_iris::Value::Str(n.name().to_owned()));
                    for key in ["position", "rotation", "scale", "visible", "z_index", "modulate"] {
                        if let Some(v) = n.get_prop(key) {
                            interp.vars.insert(key.to_owned(), godot_script_iris::variant_to_value(&v));
                        }
                    }
                }
                for p in &file.props {
                    if !interp.vars.contains_key(&p.name) {
                        let v = godot_script_iris::eval_expr(&p.default, &interp.vars)
                            .unwrap_or(godot_script_iris::Value::Str(p.default.clone()));
                        interp.vars.insert(p.name.clone(), v);
                    }
                }
                interp.run(&func.body);
                self.script_output.entry(id).or_default().extend(interp.output);
                // Write back moved properties.
                let changed: Vec<(String, godot_script_iris::Value)> = interp
                    .vars
                    .iter()
                    .filter(|(k, _)| ["position", "rotation", "scale", "visible", "z_index", "modulate"].contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (k, v) in changed {
                    let _ = self.set_prop(ObjectId(id), &k, godot_script_iris::value_to_variant(&v));
                }
            }
        }
    }
    pub fn push_call(&mut self, target: ObjectId, method: impl Into<String>, args: Vec<Variant>) {
        self.queue.push_call(target, method, args);
    }
    pub fn call_method(&mut self, target: ObjectId, method: &str, args: &[Variant]) -> Result<Variant, GodotError> {
        // Native method surface for v1 (script methods dispatch in module 05).
        match method {
            "queue_free" => {
                self.queue_free(target)?;
                Ok(Variant::Nil)
            }
            "add_to_group" => {
                if let Some(Variant::String(g)) = args.first() {
                    self.add_to_group(target, g)?;
                    return Ok(Variant::Nil);
                }
                Err(GodotError::InvalidArgument("add_to_group needs a String".into()))
            }
            "set" => {
                if args.len() == 2 {
                    if let Variant::String(prop) = &args[0] {
                        self.set_prop(target, prop, args[1].clone())?;
                        return Ok(Variant::Nil);
                    }
                }
                Err(GodotError::InvalidArgument("set(prop, value)".into()))
            }
            "get" => {
                if let Some(Variant::String(prop)) = args.first() {
                    return self.get_prop(target, prop);
                }
                Err(GodotError::InvalidArgument("get(prop)".into()))
            }
            _ => Err(GodotError::NotFound(format!("method {method}"))),
        }
    }

    // ------------------------------------------------------- scripts (Iris)
    pub fn attach_script(&mut self, id: ObjectId, source: impl Into<String>) -> Result<(), GodotError> {
        if !self.nodes.contains_key(&id.0) {
            return Err(GodotError::NotFound(format!("node {}", id.0)));
        }
        self.scripts.insert(id.0, source.into());
        Ok(())
    }
    pub fn script_of(&self, id: ObjectId) -> Option<&str> {
        self.scripts.get(&id.0).map(|s| s.as_str())
    }
    /// Captured `print` output of a node's script (cleared per test via `clear_script_output`).
    pub fn script_output(&self, id: ObjectId) -> &[String] {
        self.script_output.get(&id.0).map(|v| v.as_slice()).unwrap_or(&[])
    }
    pub fn clear_script_output(&mut self, id: ObjectId) {
        if let Some(v) = self.script_output.get_mut(&id.0) {
            v.clear();
        }
    }
    /// Enable per-frame `_process` / `_physics_process` for a node (Godot
    /// enables this via `set_process(true)` when a script defines them).
    pub fn enable_process(&mut self, id: ObjectId, idle: bool, physics: bool) -> Result<(), GodotError> {
        let n = self.get_stored_mut(id)?;
        n.data_mut().processing = idle;
        n.data_mut().physics_processing = physics;
        Ok(())
    }

    // ------------------------------------------------------- MCP bridge ----
    /// `inspect` payload for `godot_ai::McpServer` (ClassDB + live tree).
    pub fn inspect_summary(&self) -> serde_json::Value {
        let db = godot_core::global_db();
        let nodes: Vec<serde_json::Value> = self
            .nodes
            .values()
            .map(|n| {
                serde_json::json!({
                    "id": n.id().0,
                    "name": n.name(),
                    "class": n.class(),
                    "parent": n.data().parent.map(|p| p.0),
                    "children": n.data().children.iter().map(|c| c.0).collect::<Vec<_>>(),
                    "groups": n.data().groups.iter().collect::<Vec<_>>(),
                })
            })
            .collect();
        serde_json::json!({
            "engine": godot_core::ENGINE_VERSION,
            "classes": db.class_names(),
            "root": self.root_id,
            "node_count": self.nodes.len(),
            "frame": self.idle_frame,
            "paused": self.paused,
            "nodes": nodes,
        })
    }

    /// `execute` dispatch for `godot_ai::McpServer` (mirrors SolidCraft tools).
    pub fn execute(&mut self, command: &str, args: &serde_json::Value) -> Result<serde_json::Value, GodotError> {
        let get_id = |key: &str| -> Result<ObjectId, GodotError> {
            args.get(key)
                .and_then(|v| v.as_u64())
                .map(ObjectId)
                .ok_or_else(|| GodotError::InvalidArgument(format!("missing u64 `{key}`")))
        };
        match command {
            "add_node" => {
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Node");
                let class = args.get("class").and_then(|v| v.as_str()).unwrap_or("Node");
                let parent = args.get("parent").and_then(|v| v.as_u64()).map(ObjectId);
                let id = match class {
                    "Node2D" => self.add_2d(Node2D::new(name), parent)?,
                    "Node3D" => self.add_3d(Node3D::new(name), parent)?,
                    _ => self.add_base(Node::new(name), parent)?,
                };
                Ok(serde_json::json!({"id": id.0}))
            }
            "set_parameter" | "set_prop" => {
                let id = get_id("node")?;
                let prop = args.get("property").and_then(|v| v.as_str()).ok_or_else(|| GodotError::InvalidArgument("missing property".into()))?;
                let value = args.get("value").cloned().unwrap_or(serde_json::Value::Null);
                let v = json_to_variant(&value);
                self.set_prop(id, prop, v)?;
                Ok(serde_json::json!({"ok": true}))
            }
            "get_prop" => {
                let id = get_id("node")?;
                let prop = args.get("property").and_then(|v| v.as_str()).ok_or_else(|| GodotError::InvalidArgument("missing property".into()))?;
                let v = self.get_prop(id, prop)?;
                Ok(serde_json::json!({"value": variant_to_json(&v)}))
            }
            "remove_node" | "free" => {
                let id = get_id("node")?;
                self.free(id)?;
                Ok(serde_json::json!({"ok": true}))
            }
            "list" => Ok(serde_json::json!({"nodes": self.inspect_summary()["nodes"], "tree": self.print_tree_pretty()})),
            "tree" => Ok(serde_json::json!({"tree": self.print_tree_pretty()})),
            _ => Err(GodotError::NotFound(format!("unknown command {command}"))),
        }
    }

    // ------------------------------------------------------- (de)serialize
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.nodes).unwrap_or_else(|_| "{}".into())
    }
}

impl Default for SceneTree {
    fn default() -> Self {
        Self::new()
    }
}

fn glob_match(pattern: &str, name: &str) -> bool {
    // `*` (any run) + `?` (single char), case-sensitive like Godot.
    let (px, nx) = (pattern.as_bytes(), name.as_bytes());
    let (mut star, mut mark) = (None::<usize>, 0usize);
    let (mut i, mut j) = (0usize, 0usize);
    while j < nx.len() {
        if i < px.len() && (px[i] == b'?' || px[i] == nx[j]) {
            i += 1;
            j += 1;
        } else if i < px.len() && px[i] == b'*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if star.is_some() {
            i = star.unwrap() + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < px.len() && px[i] == b'*' {
        i += 1;
    }
    i == px.len()
}

pub(crate) fn json_to_variant(v: &serde_json::Value) -> Variant {
    use godot_core::{GodotArray, GodotDictionary};
    match v {
        serde_json::Value::Null => Variant::Nil,
        serde_json::Value::Bool(b) => Variant::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Variant::Int(i)
            } else {
                Variant::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Variant::String(s.clone()),
        serde_json::Value::Array(a) => {
            let mut arr = GodotArray::new();
            for item in a.iter().map(json_to_variant) {
                let _ = arr.push(item);
            }
            Variant::Array(arr)
        }
        serde_json::Value::Object(m) => {
            let mut d = GodotDictionary::new();
            for (k, vv) in m {
                let _ = d.set(k.clone(), json_to_variant(vv));
            }
            Variant::Dictionary(d)
        }
    }
}

pub(crate) fn variant_to_json(v: &Variant) -> serde_json::Value {
    match v {
        Variant::Nil => serde_json::Value::Null,
        Variant::Bool(b) => serde_json::json!(*b),
        Variant::Int(i) => serde_json::json!(*i),
        Variant::Float(f) => serde_json::json!(*f),
        Variant::String(s) | Variant::StringName(s) => serde_json::json!(s),
        Variant::Vector2(p) => serde_json::json!([p.x, p.y]),
        Variant::Vector3(p) => serde_json::json!([p.x, p.y, p.z]),
        other => serde_json::json!(other.to_string_lossy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tree2() -> (SceneTree, ObjectId, ObjectId) {
        let mut t = SceneTree::new();
        let root = t.root_id();
        let a = t.add_base(Node::new("A"), None).unwrap();
        let b = t.add_2d(Node2D::new("B"), Some(a)).unwrap();
        let _ = (root, b);
        (t, a, b)
    }
    #[test]
    fn add_validates_names_like_godot() {
        let mut t = SceneTree::new();
        let a = t.add_base(Node::new("A"), None).unwrap();
        let dup = t.add_base(Node::new("A"), Some(t.root_id())).unwrap();
        assert_eq!(t.get_stored(dup).unwrap().name(), "A@2");
        assert!(t.get_stored(a).is_ok());
    }
    #[test]
    fn paths_resolve_like_godot() {
        let (t, a, b) = tree2();
        let by_abs = t.get_node_by_path(t.root_id(), &NodePath::new("/root/A/B")).unwrap();
        assert_eq!(by_abs, b);
        let by_rel = t.get_node_by_path(a, &NodePath::new("B")).unwrap();
        assert_eq!(by_rel, b);
        let up = t.get_node_by_path(b, &NodePath::new("..")).unwrap();
        assert_eq!(up, a);
        let path = t.get_path(b).unwrap();
        assert_eq!(path.raw, "/root/A/B");
    }
    #[test]
    fn free_cascades_and_groups_work() {
        let mut t = SceneTree::new();
        let a = t.add_base(Node::new("A"), None).unwrap();
        let b = t.add_base(Node::new("B"), Some(a)).unwrap();
        t.add_to_group(b, "enemies").unwrap();
        assert_eq!(t.get_nodes_in_group("enemies"), vec![b]);
        t.call_group_set("enemies", "hp", Variant::Int(10));
        assert_eq!(t.get_prop(b, "hp").unwrap(), Variant::Int(10));
        t.free(a).unwrap();
        assert!(!t.contains(a) && !t.contains(b));
        assert!(t.get_nodes_in_group("enemies").is_empty());
    }
    #[test]
    fn timers_queue_and_physics_advance() {
        let mut t = SceneTree::new();
        let id = t.create_timer(1.0, true, true);
        let e1 = t.process_frame(0.5);
        assert!(e1.is_empty());
        let e2 = t.process_frame(0.6);
        assert_eq!(e2, vec![id]);
        t.queue_free(t.root_id()).ok();
        // root free is rejected; queue flag path still observable:
        let a = t.add_base(Node::new("Tmp"), None).unwrap();
        t.queue_free(a).unwrap();
        t.process_frame(0.0);
        assert!(!t.contains(a));
        t.physics_frame(1.0 / 60.0);
        assert_eq!(t.physics_frame, 1);
    }
    #[test]
    fn execute_bridge_matches_mcp_contract() {
        let mut t = SceneTree::new();
        let r = t.execute("add_node", &serde_json::json!({"name":"S","class":"Node3D"})).unwrap();
        let id = ObjectId(r["id"].as_u64().unwrap());
        t.execute("set_parameter", &serde_json::json!({"node": id.0, "property":"visible","value": false})).unwrap();
        assert_eq!(t.get_prop(id, "visible").unwrap(), Variant::Bool(false));
    }
    #[test]
    fn iris_ready_and_process_run_per_frame() {
        let mut t = SceneTree::new();
        let a = t.add_base(Node::new("A"), None).unwrap();
        t.attach_script(
            a,
            "node A : Node {\nfn _ready() {\nprint booted\n}\nfn _process(dt: float) {\nprint tick\n}\n}\n",
        )
        .unwrap();
        t.enable_process(a, true, false).unwrap();
        t.process_frame(1.0 / 60.0);
        // _ready once + first _process.
        assert_eq!(t.script_output(a), &["booted".to_string(), "tick".to_string()]);
        t.process_frame(1.0 / 60.0);
        // Only _process on later frames.
        assert_eq!(t.script_output(a), &["booted".to_string(), "tick".to_string(), "tick".to_string()]);
    }
    #[test]
    fn iris_physics_gated_by_flag() {
        let mut t = SceneTree::new();
        let a = t.add_base(Node::new("A"), None).unwrap();
        t.attach_script(a, "node A : Node {\nfn _physics_process(dt: float) {\nprint step\n}\n}\n")
            .unwrap();
        t.physics_frame(1.0 / 60.0);
        assert!(t.script_output(a).is_empty()); // physics_processing off by default
        t.enable_process(a, false, true).unwrap();
        t.physics_frame(1.0 / 60.0);
        assert_eq!(t.script_output(a), &["step".to_string()]);
    }
    #[test]
    fn iris_moves_node_through_props() {
        use godot_core::Vector2;
        let mut t = SceneTree::new();
        let mut n = Node2D::new("Mover");
        n.set_position(Vector2::new(0.0, 0.0));
        let id = t.add_2d(n, None).unwrap();
        t.attach_script(
            id,
            "node Mover : Node2D {\nprop speed: float = 200.0\nfn _process(dt: float) {\nposition.x += speed * dt\n}\n}\n",
        )
        .unwrap();
        t.enable_process(id, true, false).unwrap();
        t.process_frame(0.5);
        let pos = match t.get_prop(id, "position").unwrap() {
            Variant::Vector2(p) => p,
            other => panic!("expected Vector2, got {other:?}"),
        };
        assert!((pos.x - 100.0).abs() < 1e-3, "moved by speed*dt, got {pos:?}");
    }
    #[test]
    fn paused_tree_skips_scripts_and_timers() {
        let mut t = SceneTree::new();
        let a = t.add_base(Node::new("A"), None).unwrap();
        t.attach_script(a, "node A : Node {\nfn _process(dt: float) {\nprint tick\n}\n}\n").unwrap();
        t.enable_process(a, true, false).unwrap();
        t.paused = true;
        assert!(t.process_frame(1.0).is_empty());
        assert!(t.script_output(a).is_empty());
        t.paused = false;
        t.process_frame(1.0 / 60.0);
        assert!(!t.script_output(a).is_empty());
    }
}
