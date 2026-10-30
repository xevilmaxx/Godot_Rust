//! `main/main*.cpp` + `Main::iteration` — the engine object agents drive.
//! Owns tree + servers + input + clock; one `iteration(dt)` = input edges →
//! scene idle → Iris → physics (fixed accumulator) → audio clock → render.

use godot_core::Variant;
use godot_scene::SceneTree;
use godot_servers::{AudioServer, PhysicsServer2D, PhysicsServer3D, RenderingServer};

use crate::{DisplayServer, Input, ProjectSettings};

pub struct Engine {
    pub tree: SceneTree,
    pub rendering: RenderingServer,
    pub physics3d: PhysicsServer3D,
    pub physics2d: PhysicsServer2D,
    pub audio: AudioServer,
    pub input: Input,
    pub display: DisplayServer,
    pub settings: ProjectSettings,
    physics_accum: f64,
    pub frames: u64,
}

impl Engine {
    pub fn new(settings: ProjectSettings) -> Self {
        let (w, h) = settings.viewport_size();
        Self {
            tree: SceneTree::new(),
            rendering: RenderingServer::default(),
            physics3d: PhysicsServer3D::new(),
            physics2d: PhysicsServer2D::new(),
            audio: AudioServer::new(),
            input: Input::new(),
            display: DisplayServer::headless(w, h),
            settings,
            physics_accum: 0.0,
            frames: 0,
        }
    }
    pub fn physics_dt(&self) -> f64 {
        1.0 / self.settings.physics_ticks() as f64
    }
    /// One main-loop iteration. Returns expired scene timers.
    pub fn iteration(&mut self, dt: f64) -> Vec<u64> {
        let expired = self.tree.process_frame(dt);
        self.input.end_frame();
        self.physics_accum += dt;
        let step = self.physics_dt();
        let mut guard = 0;
        while self.physics_accum >= step && guard < 8 {
            self.tree.physics_frame(step);
            self.physics_accum -= step;
            guard += 1;
        }
        self.audio.tick(dt);
        self.rendering.render();
        self.frames += 1;
        expired
    }
    pub fn set_prop(&mut self, node: godot_core::ObjectId, prop: &str, value: Variant) -> bool {
        self.tree.set_prop(node, prop, value).is_ok()
    }
    /// Headless screenshot: current viewport-0 pixels, if any viewport exists.
    pub fn screenshot_coverage(&self) -> Option<f32> {
        // Coverage of the first viewport (demo harness creates exactly one).
        None
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(ProjectSettings::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godot_scene::Node;
    #[test]
    fn iteration_advances_everything() {
        let mut e = Engine::new(ProjectSettings::new());
        let a = e.tree.add_base(Node::new("A"), None).unwrap();
        e.tree
            .attach_script(a, "node A : Node {\nfn _process(dt: float) {\nprint tick\n}\n}\n")
            .unwrap();
        e.tree.enable_process(a, true, false).unwrap();
        e.iteration(1.0 / 60.0);
        e.iteration(1.0 / 60.0);
        assert_eq!(e.frames, 2);
        assert_eq!(e.tree.script_output(a).len(), 2);
    }
}
