# godot_scene — `scene/main` + Node2D/3D + PackedScene (module 02, COMPLETE)

Port of `scene/main/node.h/.cpp`, `scene_tree.h/.cpp`, `timer_process.h`,
`scene/2d/node_2d.h`, `scene/3d/node_3d.h`, `scene/resources/packed_scene.h`
(JSON form).

## What’s inside

- **Tree**: unique sibling names (`A`, `A@2`), add/remove/reparent, path resolution
  (`/root/A/B`, `..`, `.`, `%Unique`), `get_path`, `find_child` globs,
  `print_tree_pretty`, `duplicate`, `queue_free` cascade, groups, process modes.
- **Node2D/Node3D**: local/global transforms, `to_local`/`to_global`, `look_at`
  (2D rotation / 3D −Z convention), visibility, `rotated_local` (3D).
- **SceneTree**: registry, pause/time-scale, idle+physics frames, timers
  (`create_timer`), deferred calls, `call_method`/`call_group_set`,
  Iris `attach_script` (preset/write-back of `position/rotation/scale/...`),
  MCP `inspect_summary` + `execute` (add_node/set_parameter/get_prop/free/list/tree).
- **PackedScene**: pack/instantiate/JSON roundtrip with props + groups.

## Example

```rust
let mut t = SceneTree::new();
let a = t.add_base(Node::new("Player"), None)?;
t.attach_script(a, "node P : Node {\nfn _process(dt: float) {\nprint dt\n}\n}\n")?;
t.enable_process(a, true, false)?;
t.process_frame(1.0 / 60.0);
```

## Verify

`cargo test -p godot_scene` — 15 tests.
