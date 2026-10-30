//! Port of `servers/navigation_2d/navigation_server_2d.h` (grid query model).

use godot_core::{Rid, Vector2};
use std::collections::{BTreeMap, BinaryHeap};

/// Minimal typed owner (mirrors `servers` `RidOwner`; nav maps are few).
#[derive(Debug, Default)]
struct Owner<T> {
    next: u64,
    items: BTreeMap<u64, T>,
}
impl<T> Owner<T> {
    fn new() -> Self {
        Self { next: 0, items: BTreeMap::new() }
    }
    fn alloc(&mut self, v: T) -> Rid {
        self.next += 1;
        self.items.insert(self.next, v);
        Rid(self.next)
    }
    fn get(&self, id: Rid) -> Option<&T> {
        self.items.get(&id.0)
    }
    fn get_mut(&mut self, id: Rid) -> Option<&mut T> {
        self.items.get_mut(&id.0)
    }
    fn free(&mut self, id: Rid) -> bool {
        self.items.remove(&id.0).is_some()
    }
}

#[derive(Debug, Clone)]
struct GridMap {
    width: u32,
    height: u32,
    cell: f32,
    blocked: Vec<bool>,
    diagonal: bool,
}

impl GridMap {
    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some((y as u32 * self.width + x as u32) as usize)
    }
    fn to_cell(&self, p: Vector2) -> (i32, i32) {
        ((p.x / self.cell).floor() as i32, (p.y / self.cell).floor() as i32)
    }
    fn to_world(&self, x: i32, y: i32) -> Vector2 {
        Vector2::new((x as f32 + 0.5) * self.cell, (y as f32 + 0.5) * self.cell)
    }
    fn passable(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).map(|i| !self.blocked[i]).unwrap_or(false)
    }
}

pub struct NavigationServer2D {
    maps: Owner<GridMap>,
}

impl NavigationServer2D {
    pub fn new() -> Self {
        Self { maps: Owner::new() }
    }
    pub fn map_create(&mut self, width: u32, height: u32, cell: f32, diagonal: bool) -> Rid {
        let (w, h) = (width.max(1), height.max(1));
        self.maps.alloc(GridMap { width: w, height: h, cell: cell.max(0.01), blocked: vec![false; (w * h) as usize], diagonal })
    }
    pub fn map_set_blocked(&mut self, map: Rid, x: u32, y: u32, blocked: bool) -> bool {
        if let Some(m) = self.maps.get_mut(map) {
            if let Some(i) = m.idx(x as i32, y as i32) {
                m.blocked[i] = blocked;
                return true;
            }
        }
        false
    }
    pub fn map_free(&mut self, map: Rid) -> bool {
        self.maps.free(map)
    }
    /// Bake world-space obstacle rects `(center, extents)` into blocked cells.
    /// Cells whose center falls inside any rect become walls.
    pub fn bake_rects(&mut self, map: Rid, rects: &[(Vector2, Vector2)]) -> usize {
        let Some(m) = self.maps.get_mut(map) else { return 0 };
        let mut n = 0;
        for y in 0..m.height as i32 {
            for x in 0..m.width as i32 {
                let c = m.to_world(x, y);
                if rects.iter().any(|(cc, e)| (c.x - cc.x).abs() <= e.x && (c.y - cc.y).abs() <= e.y) {
                    if let Some(i) = m.idx(x, y) {
                        if !m.blocked[i] {
                            m.blocked[i] = true;
                            n += 1;
                        }
                    }
                }
            }
        }
        n
    }
    pub fn blocked_count(&self, map: Rid) -> usize {
        self.maps.get(map).map(|m| m.blocked.iter().filter(|&&b| b).count()).unwrap_or(0)
    }
    /// A* from `from` to `to` (world coords); returns world-space waypoints
    /// including start/end, string-pulled. Empty = unreachable.
    pub fn map_get_path(&self, map: Rid, from: Vector2, to: Vector2) -> Vec<Vector2> {
        let Some(m) = self.maps.get(map) else { return vec![] };
        let (sx, sy) = m.to_cell(from);
        let (gx, gy) = m.to_cell(to);
        if !m.passable(gx, gy) {
            return vec![];
        }
        let start = if m.passable(sx, sy) { (sx, sy) } else { return vec![from, to] };
        let raw = astar(m, start, (gx, gy));
        if raw.is_empty() {
            return vec![];
        }
        let mut pts: Vec<Vector2> = raw.iter().map(|&(x, y)| m.to_world(x, y)).collect();
        if !pts.is_empty() {
            pts[0] = from;
            let last = pts.len() - 1;
            pts[last] = to;
        }
        string_pull(m, &pts)
    }
}

impl Default for NavigationServer2D {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct State {
    f: f32,
    x: i32,
    y: i32,
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

fn astar(m: &GridMap, start: (i32, i32), goal: (i32, i32)) -> Vec<(i32, i32)> {
    let h = |p: (i32, i32)| ((p.0 - goal.0).abs() + (p.1 - goal.1).abs()) as f32;
    let mut open = BinaryHeap::new();
    let mut g: BTreeMap<(i32, i32), f32> = BTreeMap::new();
    let mut came: BTreeMap<(i32, i32), (i32, i32)> = BTreeMap::new();
    g.insert(start, 0.0);
    open.push(State { f: h(start), x: start.0, y: start.1 });
    let dirs: &[(i32, i32, f32)] = if m.diagonal {
        &[(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0), (1, 1, 1.4142), (1, -1, 1.4142), (-1, 1, 1.4142), (-1, -1, 1.4142)]
    } else {
        &[(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0)]
    };
    while let Some(State { x, y, .. }) = open.pop() {
        if (x, y) == goal {
            let mut path = vec![(x, y)];
            let mut cur = (x, y);
            while let Some(&p) = came.get(&cur) {
                path.push(p);
                cur = p;
            }
            path.reverse();
            return path;
        }
        let cur_g = g.get(&(x, y)).copied().unwrap_or(f32::INFINITY);
        for (dx, dy, cost) in dirs {
            let (nx, ny) = (x + dx, y + dy);
            if !m.passable(nx, ny) {
                continue;
            }
            // No corner cutting on diagonals.
            if *dx != 0 && *dy != 0 && (!m.passable(x + dx, y) || !m.passable(x, y + dy)) {
                continue;
            }
            let ng = cur_g + cost;
            if ng < g.get(&(nx, ny)).copied().unwrap_or(f32::INFINITY) {
                g.insert((nx, ny), ng);
                came.insert((nx, ny), (x, y));
                open.push(State { f: ng + h((nx, ny)), x: nx, y: ny });
            }
        }
    }
    vec![]
}

/// Greedy line-of-sight smoothing (funnel-lite): keep furthest visible waypoint.
fn string_pull(m: &GridMap, pts: &[Vector2]) -> Vec<Vector2> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    let mut anchor = 0;
    let mut i = 2;
    while i < pts.len() {
        if los(m, pts[anchor], pts[i]) {
            i += 1;
        } else {
            out.push(pts[i - 1]);
            anchor = i - 1;
            i = anchor + 2;
        }
    }
    out.push(*pts.last().unwrap());
    out
}

fn los(m: &GridMap, a: Vector2, b: Vector2) -> bool {
    // Sample along the segment at half-cell steps.
    let dist = (b - a).length();
    let steps = (dist / (m.cell * 0.5)).ceil().max(1.0) as usize;
    for i in 0..=steps.min(4096) {
        let p = a + (b - a) * (i as f32 / steps as f32);
        let (cx, cy) = m.to_cell(p);
        if !m.passable(cx, cy) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn straight_path_without_obstacles() {
        let mut n = NavigationServer2D::new();
        let map = n.map_create(10, 10, 1.0, true);
        let path = n.map_get_path(map, Vector2::new(0.5, 0.5), Vector2::new(8.5, 8.5));
        assert!(path.len() >= 2);
        assert_eq!(path[0], Vector2::new(0.5, 0.5));
        assert_eq!(*path.last().unwrap(), Vector2::new(8.5, 8.5));
    }
    #[test]
    fn wall_forces_detour_and_pulling_keeps_endpoints() {
        let mut n = NavigationServer2D::new();
        let map = n.map_create(10, 10, 1.0, true);
        for y in 0..9 {
            n.map_set_blocked(map, 5, y, true); // vertical wall with gap at y=9
        }
        let path = n.map_get_path(map, Vector2::new(0.5, 4.5), Vector2::new(9.5, 4.5));
        assert!(path.len() >= 3, "must route around, got {path:?}");
        for p in &path {
            let (cx, cy) = (p.x.floor() as i32, p.y.floor() as i32);
            assert!(!(cx == 5 && cy < 9), "path enters wall at {p:?}");
        }
        assert_eq!(*path.last().unwrap(), Vector2::new(9.5, 4.5));
    }
    #[test]
    fn bake_rects_blocks_cells() {
        let mut n = NavigationServer2D::new();
        let map = n.map_create(10, 10, 1.0, true);
        let blocked = n.bake_rects(map, &[(Vector2::new(4.5, 4.5), Vector2::new(1.0, 4.5))]);
        assert!(blocked > 5, "wall column baked, got {blocked}");
        assert_eq!(n.blocked_count(map), blocked);
        // Baked wall forces a detour like the hand-painted one.
        let path = n.map_get_path(map, Vector2::new(0.5, 4.5), Vector2::new(9.5, 4.5));
        assert!(path.len() >= 3, "must route around baked wall, got {path:?}");
    }
    #[test]
    fn blocked_goal_is_unreachable() {
        let mut n = NavigationServer2D::new();
        let map = n.map_create(4, 4, 1.0, false);
        n.map_set_blocked(map, 3, 3, true);
        assert!(n.map_get_path(map, Vector2::new(0.5, 0.5), Vector2::new(3.5, 3.5)).is_empty());
    }
}
