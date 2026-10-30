# godot_nav — navigation servers (module 09, COMPLETE)

Port of `servers/navigation_2d`, `servers/navigation_3d`,
`modules/navigation_2d/3d` (query-side). Baked-mesh import is the documented
non-goal (09b); the query API matches so baked data drops in later.

## What’s inside

- **`nav2d`**: occupancy-grid maps (4/8-connectivity), obstacle painting,
  A* (no corner cutting), line-of-sight string pulling, unreachable → empty.
- **`nav3d`**: waypoint-graph maps (vertices + edge pairs, Godot’s baked-mesh
  equivalent at query level), closest-vertex A*, collinear cleanup.

## Example

```rust
let map = nav.map_create(10, 10, 1.0, true);
let path: Vec<Vector2> = nav.map_get_path(map, from, to); // start..end, smoothed
```

## Baking (query-side, no Recast needed)

- `bake_rects`: rasterize world-space AABB obstacles into grid blocked cells.
- `map_create_from_tris`: waypoint graph from triangles — walkable when
  `|slope normal.y| >= min_up_dot`, vertices welded at 1mm, centroid nodes
  linked across shared edges. Feed it `GltfDocument` mesh triangles.

## Verify

`cargo test -p godot_nav` — detour, unreachable, ring graph, baking.
