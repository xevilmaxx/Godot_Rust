//! Port of `servers/display/display_server.h` (headless subset).
//! Real windowing (winit + wgpu surface) is the one deliberate gap: it needs a
//! compositor at runtime, so headless CI + agents get a faithful stub with the
//! same API shape. `has_feature(DisplayServer::FEATURE_WINDOW)` reports false.

#[derive(Debug, Clone)]
pub struct DisplayServer {
    title: String,
    width: u32,
    height: u32,
    headless: bool,
}

impl DisplayServer {
    pub fn headless(width: u32, height: u32) -> Self {
        Self { title: "godot_rs".into(), width, height, headless: true }
    }
    pub fn name(&self) -> &'static str {
        if self.headless {
            "headless"
        } else {
            "window"
        }
    }
    pub fn is_headless(&self) -> bool {
        self.headless
    }
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_owned();
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn window_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn set_window_size(&mut self, w: u32, h: u32) {
        self.width = w.max(1);
        self.height = h.max(1);
    }
    pub fn screen_count(&self) -> usize {
        1
    }
}

impl Default for DisplayServer {
    fn default() -> Self {
        Self::headless(1152, 648)
    }
}
