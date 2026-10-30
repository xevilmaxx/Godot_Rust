//! Port of `scene/resources/animation.h` + `scene/animation/animation_player.h`
//! (value-track subset): float/bool keyframes over node properties, loop modes,
//! blend by weight. Transform tracks bake down to property paths
//! (`position`, `rotation`, `modulate:a`, …) like Godot's generic tracks.

use godot_core::{GodotError, Variant};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoopMode {
    #[default]
    None,
    Linear,
    PingPong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub time: f64,
    pub value: Variant,
    pub transition: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Track {
    /// Property path, e.g. `position`, `modulate:a`, `visible`.
    pub path: String,
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Animation {
    pub name: String,
    pub length: f64,
    pub loop_mode: LoopMode,
    pub tracks: Vec<Track>,
}

impl Animation {
    pub fn new(name: &str, length: f64) -> Self {
        Self { name: name.into(), length: length.max(0.001), loop_mode: LoopMode::None, tracks: vec![] }
    }
    pub fn add_track(&mut self, path: &str) -> usize {
        self.tracks.push(Track { path: path.into(), keys: vec![] });
        self.tracks.len() - 1
    }
    pub fn track_add_key(&mut self, track: usize, time: f64, value: Variant) -> Result<(), GodotError> {
        let t = self.tracks.get_mut(track).ok_or_else(|| GodotError::NotFound("track".into()))?;
        t.keys.push(Key { time: time.clamp(0.0, self.length), value, transition: 1.0 });
        t.keys.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        Ok(())
    }
    /// Sample a track at `time` (linear float lerp; discrete for the rest).
    pub fn sample(&self, track: usize, mut time: f64) -> Option<Variant> {
        let t = self.tracks.get(track)?;
        if t.keys.is_empty() {
            return None;
        }
        time = match self.loop_mode {
            LoopMode::None => time.clamp(0.0, self.length),
            LoopMode::Linear => time.rem_euclid(self.length.max(1e-9)),
            LoopMode::PingPong => {
                let l = self.length.max(1e-9);
                let c = time.rem_euclid(l * 2.0);
                if c > l {
                    l * 2.0 - c
                } else {
                    c
                }
            }
        };
        let mut prev = &t.keys[0];
        if time <= prev.time {
            return Some(prev.value.clone());
        }
        for k in &t.keys[1..] {
            if time <= k.time {
                let span = (k.time - prev.time).max(1e-9);
                let w = ((time - prev.time) / span) as f32;
                return Some(lerp_variant(&prev.value, &k.value, w));
            }
            prev = k;
        }
        Some(prev.value.clone())
    }
}

fn lerp_variant(a: &Variant, b: &Variant, w: f32) -> Variant {
    match (a, b) {
        (Variant::Float(x), Variant::Float(y)) => Variant::Float(x + (y - x) * w as f64),
        (Variant::Int(x), Variant::Int(y)) => Variant::Float(*x as f64 + (*y as f64 - *x as f64) * w as f64),
        (Variant::Vector2(x), Variant::Vector2(y)) => Variant::Vector2(*x + (*y - *x) * w),
        (Variant::Vector3(x), Variant::Vector3(y)) => Variant::Vector3(*x + (*y - *x) * w),
        (Variant::Color(x), Variant::Color(y)) => Variant::Color(x.lerp(*y, w)),
        _ => {
            if w < 0.5 {
                a.clone()
            } else {
                b.clone()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Playing {
    pub anim: String,
    pub position: f64,
    pub speed: f32,
    pub weight: f32,
}

/// Applies animations to a property sink (`SceneTree::set_prop`-shaped).
/// The sink receives `(node_name, property_path, value)`.
pub struct AnimationPlayer {
    library: BTreeMap<String, Animation>,
    playing: Option<Playing>,
    pub autoplay: Option<String>,
}

impl AnimationPlayer {
    pub fn new() -> Self {
        Self { library: BTreeMap::new(), playing: None, autoplay: None }
    }
    pub fn add_animation(&mut self, anim: Animation) {
        self.library.insert(anim.name.clone(), anim);
    }
    pub fn has_animation(&self, name: &str) -> bool {
        self.library.contains_key(name)
    }
    pub fn play(&mut self, name: &str, speed: f32) -> Result<(), GodotError> {
        if !self.library.contains_key(name) {
            return Err(GodotError::NotFound(format!("animation {name}")));
        }
        self.playing = Some(Playing { anim: name.into(), position: 0.0, speed, weight: 1.0 });
        Ok(())
    }
    pub fn stop(&mut self) {
        self.playing = None;
    }
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }
    /// Name of the currently playing (or last started) animation, if any.
    pub fn current(&self) -> Option<String> {
        self.playing.as_ref().map(|p| p.anim.clone())
    }
    pub fn position(&self) -> f64 {
        self.playing.as_ref().map(|p| p.position).unwrap_or(0.0)
    }
    /// Advance; returns sampled `(track_path, value)` pairs for the caller to apply.
    pub fn advance(&mut self, dt: f64) -> Vec<(String, Variant)> {
        let Some(pl) = self.playing.as_mut() else { return vec![] };
        let Some(anim) = self.library.get(&pl.anim) else { return vec![] };
        pl.position += dt * pl.speed as f64;
        if anim.loop_mode == LoopMode::None && pl.position >= anim.length {
            pl.position = anim.length;
            let out: Vec<(String, Variant)> =
                anim.tracks.iter().enumerate().filter_map(|(i, t)| anim.sample(i, pl.position).map(|v| (t.path.clone(), v))).collect();
            self.playing = None; // finished
            return out;
        }
        anim.tracks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| anim.sample(i, pl.position).map(|v| (t.path.clone(), v)))
            .collect()
    }
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_sample_and_loop() {
        let mut a = Animation::new("move", 2.0);
        let t = a.add_track("position:x");
        a.track_add_key(t, 0.0, Variant::Float(0.0)).unwrap();
        a.track_add_key(t, 2.0, Variant::Float(10.0)).unwrap();
        assert_eq!(a.sample(t, 1.0), Some(Variant::Float(5.0)));
        a.loop_mode = LoopMode::Linear;
        assert_eq!(a.sample(t, 3.0), Some(Variant::Float(5.0)));
        a.loop_mode = LoopMode::PingPong;
        assert_eq!(a.sample(t, 3.0), Some(Variant::Float(5.0)));
        assert_eq!(a.sample(t, 4.0), Some(Variant::Float(0.0)));
    }
    #[test]
    fn player_applies_and_finishes() {
        let mut a = Animation::new("fade", 1.0);
        let t = a.add_track("modulate:a");
        a.track_add_key(t, 0.0, Variant::Float(1.0)).unwrap();
        a.track_add_key(t, 1.0, Variant::Float(0.0)).unwrap();
        let mut p = AnimationPlayer::new();
        p.add_animation(a);
        p.play("fade", 1.0).unwrap();
        let first = p.advance(0.5);
        assert_eq!(first, vec![("modulate:a".to_string(), Variant::Float(0.5))]);
        let _ = p.advance(0.6);
        assert!(!p.is_playing());
    }
}
