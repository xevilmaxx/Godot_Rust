//! Port of `servers/navigation_3d/navigation_server_3d.h` (graph query model).
//! Baked `NavigationMesh` polygons arrive as a waypoint graph: vertices +
//! bidirectional edges with traversal costs. A* + 3D string pulling.

use godot_core::{Rid, Vector3};
use std::collections::{BTreeMap, BinaryHeap};

#[derive(Debug, Clone)]
struct GraphMap {
    verts: Vec<Vector3>,
    edges: BTreeMap<usize, Vec<(usize, f32)>>,
}

pub struct NavigationServer3D {
    next: u64,
    maps: BTreeMap<u64, GraphMap>,
}

impl NavigationServer3D {
    pub fn new() -> Self {
        Self { next: 1, maps: BTreeMap::new() }
    }
    /// Create a map from vertices + index pairs (Godot bakes these from meshes).
    pub fn map_create(&mut self, verts: Vec<Vector3>, pairs: &[(usize, usize)]) -> Rid {
        let mut edges: BTreeMap<usize, Vec<(usize, f32)>> = BTreeMap::new();
        for (a, b) in pairs {
            if *a >= verts.len() || *b >= verts.len() || a == b {
                continue;
            }
            let cost = (verts[*a] - verts[*b]).length();
            edges.entry(*a).or_default().push((*b, cost));
            edges.entry(*b).or_default().push((*a, cost));
        }
        let id = self.next;
        self.next += 1;
        self.maps.insert(id, GraphMap { verts, edges });
        Rid(id)
    }
    /// Bake walkable triangles into a waypoint graph: one node per triangle
    /// centroid (steeper than `min_up_dot` are walls; `|y|` so winding never
    /// matters), edges between triangles sharing an edge (vertex weld at 1mm).
    /// Godot bakes `NavigationMesh` polygons; this is the query-side equivalent.
    pub fn map_create_from_tris(&mut self, tris: &[(Vector3, Vector3, Vector3)], min_up_dot: f32) -> Rid {
        use std::collections::BTreeMap as Map;
        let mut vert_index: Map<(i64, i64, i64), usize> = Map::new();
        let mut verts: Vec<Vector3> = vec![];
        let weld = |v: Vector3, verts: &mut Vec<Vector3>, index: &mut Map<(i64, i64, i64), usize>| -> usize {
            let key = ((v.x * 1000.0).round() as i64, (v.y * 1000.0).round() as i64, (v.z * 1000.0).round() as i64);
            if let Some(&i) = index.get(&key) {
                return i;
            }
            verts.push(v);
            let i = verts.len() - 1;
            index.insert(key, i);
            i
        };
        // Per-triangle welded index triples + centroids of walkable ones.
        // Node indices are assigned AFTER welding (verts.len() is final then).
        let mut tri_nodes: Vec<(usize, usize, usize)> = vec![];
        let mut centroids: Vec<Vector3> = vec![];
        for (p0, p1, p2) in tris {
            let n = (*p1 - *p0).cross(*p2 - *p0);
            if n.length_squared() < 1e-12 {
                continue; // degenerate
            }
            // Steepness only (|y| ignores winding: floors baked either way walk);
            // vertical faces are walls.
            if n.normalized().y.abs() < min_up_dot {
                continue;
            }
            let a = weld(*p0, &mut verts, &mut vert_index);
            let b = weld(*p1, &mut verts, &mut vert_index);
            let c = weld(*p2, &mut verts, &mut vert_index);
            centroids.push((*p0 + *p1 + *p2) / 3.0);
            tri_nodes.push((a, b, c));
        }
        let tri_nodes: Vec<(usize, usize, usize, usize)> = tri_nodes
            .into_iter()
            .enumerate()
            .map(|(i, (a, b, c))| (a, b, c, verts.len() + i))
            .collect();
        // Graph verts = welded verts + centroid nodes; edges centroid<->its
        // verts plus centroid<->centroid for triangles sharing an edge.
        let mut all_verts = verts;
        all_verts.extend(centroids.iter().copied());
        let mut pairs: Vec<(usize, usize)> = vec![];
        for (a, b, c, node) in &tri_nodes {
            pairs.push((*node, *a));
            pairs.push((*node, *b));
            pairs.push((*node, *c));
        }
        for i in 0..tri_nodes.len() {
            for j in (i + 1)..tri_nodes.len() {
                let (a0, b0, c0, ni) = tri_nodes[i];
                let (a1, b1, c1, nj) = tri_nodes[j];
                let shared = [a0, b0, c0].iter().filter(|v| **v == a1 || **v == b1 || **v == c1).count();
                if shared >= 2 {
                    pairs.push((ni, nj));
                }
            }
        }
        self.map_create(all_verts, &pairs)
    }
    /// A line of `n` evenly spaced waypoints (tests, corridors, debug paths).
    pub fn map_create_line(&mut self, from: Vector3, to: Vector3, n: usize) -> Rid {
        let n = n.max(2);
        let verts: Vec<Vector3> =
            (0..n).map(|i| from + (to - from) * (i as f32 / (n - 1) as f32)).collect();
        let pairs: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
        self.map_create(verts, &pairs)
    }
    pub fn map_free(&mut self, map: Rid) -> bool {
        self.maps.remove(&map.0).is_some()
    }
    fn nearest(&self, map: &GraphMap, p: Vector3) -> Option<usize> {
        map.verts.iter().enumerate().min_by(|a, b| {
            (*a.1 - p).length_squared().partial_cmp(&(*b.1 - p).length_squared()).unwrap_or(std::cmp::Ordering::Equal)
        }).map(|(i, _)| i)
    }
    /// Closest-vertex A* + string pulling; empty when disconnected.
    pub fn map_get_path(&self, map: Rid, from: Vector3, to: Vector3) -> Vec<Vector3> {
        let Some(m) = self.maps.get(&map.0) else { return vec![] };
        let (Some(s), Some(g)) = (self.nearest(m, from), self.nearest(m, to)) else { return vec![] };
        let idx = astar_graph(m, s, g);
        if idx.is_empty() {
            return vec![];
        }
        let mut pts: Vec<Vector3> = idx.iter().map(|i| m.verts[*i]).collect();
        if !pts.is_empty() {
            pts[0] = from;
            let last = pts.len() - 1;
            pts[last] = to;
        }
        pull_3d(&pts)
    }
}

impl Default for NavigationServer3D {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct State {
    f: f32,
    n: usize,
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.f.partial_cmp(&self.f).unwrap_or(std::cmp::Ordering::Equal)
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn astar_graph(m: &GraphMap, start: usize, goal: usize) -> Vec<usize> {
    let h = |n: usize| (m.verts[n] - m.verts[goal]).length();
    let mut open = BinaryHeap::new();
    let mut g: BTreeMap<usize, f32> = BTreeMap::new();
    let mut came: BTreeMap<usize, usize> = BTreeMap::new();
    g.insert(start, 0.0);
    open.push(State { f: h(start), n: start });
    while let Some(State { n, .. }) = open.pop() {
        if n == goal {
            let mut path = vec![n];
            let mut cur = n;
            while let Some(&p) = came.get(&cur) {
                path.push(p);
                cur = p;
            }
            path.reverse();
            return path;
        }
        let cur_g = g.get(&n).copied().unwrap_or(f32::INFINITY);
        if let Some(edges) = m.edges.get(&n) {
            for (nb, cost) in edges {
                let ng = cur_g + cost;
                if ng < g.get(nb).copied().unwrap_or(f32::INFINITY) {
                    g.insert(*nb, ng);
                    came.insert(*nb, n);
                    open.push(State { f: ng + h(*nb), n: *nb });
                }
            }
        }
    }
    vec![]
}

/// Remove collinear middle points (funnel-lite for graphs).
fn pull_3d(pts: &[Vector3]) -> Vec<Vector3> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let ab = b - a;
        let bc = c - b;
        // Keep b when direction changes (cross magnitude) or it backtracks (dot).
        if ab.cross(bc).length() > 1e-4 || ab.dot(bc) < 0.0 {
            out.push(b);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_path_is_direct() {
        let mut n = NavigationServer3D::new();
        let map = n.map_create_line(Vector3::ZERO, Vector3::new(10.0, 0.0, 0.0), 6);
        let path = n.map_get_path(map, Vector3::ZERO, Vector3::new(10.0, 0.0, 0.0));
        assert_eq!(path.len(), 2, "collinear middles pulled, got {path:?}");
        assert_eq!(*path.last().unwrap(), Vector3::new(10.0, 0.0, 0.0));
    }
    #[test]
    fn baked_tris_walk_and_walls_reject() {
        // Flat 2x1 slab (two tris) + one steep wall tri.
        let slab = vec![
            (Vector3::ZERO, Vector3::new(5.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 5.0)),
            (Vector3::new(5.0, 0.0, 0.0), Vector3::new(5.0, 0.0, 5.0), Vector3::new(0.0, 0.0, 5.0)),
            // Wall: vertical triangle.
            (Vector3::new(9.0, 0.0, 0.0), Vector3::new(9.0, 5.0, 0.0), Vector3::new(9.0, 0.0, 5.0)),
        ];
        let mut n = NavigationServer3D::new();
        let map = n.map_create_from_tris(&slab, 0.7);
        let path = n.map_get_path(map, Vector3::ZERO, Vector3::new(5.0, 0.0, 5.0));
        assert!(path.len() >= 2, "slab is walkable, got {path:?}");
        assert_eq!(*path.last().unwrap(), Vector3::new(5.0, 0.0, 5.0));
    }
    #[test]
    fn detour_beats_disconnect() {
        let mut n = NavigationServer3D::new();
        // Square ring: 0-1-2-3-0, query across the missing diagonal.
        let verts = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(5.0, 0.0, 0.0),
            Vector3::new(5.0, 0.0, 5.0),
            Vector3::new(0.0, 0.0, 5.0),
        ];
        let map = n.map_create(verts, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let path = n.map_get_path(map, Vector3::ZERO, Vector3::new(5.0, 0.0, 5.0));
        assert!(path.len() >= 3, "must walk the ring, got {path:?}");
        // Disconnected islands return empty.
        let map2 = n.map_create(vec![Vector3::ZERO, Vector3::new(50.0, 0.0, 0.0)], &[]);
        assert!(n.map_get_path(map2, Vector3::ZERO, Vector3::new(50.0, 0.0, 0.0)).is_empty());
    }
}
