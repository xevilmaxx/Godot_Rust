//! Godot-style GUI editor (eframe/egui): menu bar, 2D/3D/Script/Game screens,
//! Scene + FileSystem docks, sectioned Inspector + Node tab, bottom
//! Output/Debugger/Animation tabs, transform gizmo (Q/W/E/R), Play/Stop.
//! Headless `Engine` underneath — same state the CLI drives.

use eframe::egui;
use godot_core::{ObjectId, Variant};
use std::collections::HashMap;

const VW: u32 = 384;
const VH: u32 = 216;

pub fn run_gui() -> eframe::Result<()> {
    // wgpu by default (native viewport, zero CPU readback); set
    // GODOT_RS_GL=1 to force the glow renderer + readback path instead.
    let renderer = if std::env::var("GODOT_RS_GL").is_ok() { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_title("godot_rs — editor"),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "godot_rs — editor",
        opts,
        Box::new(|cc| Ok(Box::new(GuiApp::new(cc)) as Box<dyn eframe::App>)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainScreen {
    Scene2D,
    Scene3D,
    Script,
    Game,
    AssetLib,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BottomTab {
    Output,
    Debugger,
    Animation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GizmoMode {
    Select,
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone)]
struct Cam3D {
    target: godot_core::Vector3,
    yaw: f32,
    pitch: f32,
    dist: f32,
    ortho: bool,
    ortho_size: f32,
    grid: bool,
}

#[derive(Debug, Clone)]
struct Cam2D {
    center: godot_core::Vector2,
    zoom: f32,
    grid: bool,
}

/// GPU-resident 3D viewport: renders the scene with the editor window's own
/// wgpu device into a texture that egui displays directly — no CPU readback.
struct NativeView {
    backend: godot_servers::wgpu_backend::WgpuBackend,
    tex_id: Option<egui::TextureId>,
    w: u32,
    h: u32,
}

struct GuiApp {
    engine: godot_host::Engine,
    vp: godot_core::Rid,
    playing: bool,
    selected: Option<u64>,
    script_text: String,
    log: Vec<String>,
    tex: Option<egui::TextureHandle>,
    tex2d: Option<egui::TextureHandle>,
    native: Option<NativeView>,
    status: String,
    screen: MainScreen,
    bottom: BottomTab,
    gizmo: GizmoMode,
    snap_enabled: bool,
    snap_step: f32,
    cam2: Cam2D,
    cam3: Cam3D,
    create_open: bool,
    create_search: String,
    fs_dir: String,
    theme_dark: bool,
    project_name: String,
    // Animation dock.
    anims: HashMap<String, godot_scene_ext::Animation>,
    anim_player: godot_scene_ext::AnimationPlayer,
    anim_name: String,
    anim_target: Option<u64>,
    // Debugger tab.
    debug_breaks: Vec<String>,
    debug_trace: Vec<String>,
    debug_vars: Vec<(String, String)>,
    // Gizmo drag state (2D).
    drag_node: Option<u64>,
    drag_axis: Option<char>, // 'x' | 'y' | 'f' (free)
    game_actions: bool,
    game_prev: [bool; 5],
    about_open: bool,
    settings_open: bool,
    settings_text: String,
}

impl GuiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        godot_scene::register_scene_classes();
        godot_servers::register_server_classes();
        godot_host::register_host_classes();
        godot_scene_ext::register_scene_ext_classes();
        godot_net::register_net_classes();
        godot_nav::register_nav_classes();
        godot_io::register_io_classes();
        let mut engine = godot_host::Engine::new(godot_host::ProjectSettings::new());
        // Demo content: scripted mover + rendered box, like `launch`.
        let mover = engine.tree.add_2d(godot_scene::Node2D::new("Mover"), None).expect("mover");
        engine
            .tree
            .attach_script(
                mover,
                "node Mover : Node2D {\nprop speed: float = 200.0\nfn _process(dt: float) {\nposition.x += speed * dt\n}\n}\n",
            )
            .expect("script");
        engine.tree.enable_process(mover, true, false).expect("enable");
        let mesh = engine.rendering.mesh_create_box(godot_core::Vector3::new(0.5, 0.5, 0.5));
        let mat = engine.rendering.material_create(godot_core::Color::RED);
        let vp = engine.rendering.viewport_create(VW, VH, godot_core::Color::BLACK);
        let cam = engine.rendering.camera_create(70.0, 0.05, 100.0);
        engine.rendering.camera_set_transform(
            cam,
            godot_core::Transform3D::from_translation(godot_core::Vector3::new(0.0, 0.0, 3.0)),
        );
        engine.rendering.viewport_set_camera(vp, Some(cam));
        let _inst = engine.rendering.instance_create(Some(mesh), Some(mat));
        engine.rendering.render();
        let mut app = Self {
            engine,
            vp,
            playing: false,
            selected: Some(mover.0),
            script_text: "node Mover : Node2D {\nprop speed: float = 200.0\nfn _process(dt: float) {\nposition.x += speed * dt\n}\n}\n".into(),
            log: vec!["godot_rs editor ready — F5 runs the scene".into()],
            tex: None,
            tex2d: None,
            native: None,
            status: String::new(),
            screen: MainScreen::Scene3D,
            bottom: BottomTab::Output,
            gizmo: GizmoMode::Translate,
            snap_enabled: false,
            snap_step: 1.0,
            cam2: Cam2D { center: godot_core::Vector2::ZERO, zoom: 1.0, grid: true },
            cam3: Cam3D {
                target: godot_core::Vector3::ZERO,
                yaw: 0.0,
                pitch: -0.35,
                dist: 6.0,
                ortho: false,
                ortho_size: 6.0,
                grid: true,
            },
            create_open: false,
            create_search: String::new(),
            fs_dir: "res://".into(),
            theme_dark: true,
            project_name: "godot_rs".into(),
            anims: HashMap::new(),
            anim_player: godot_scene_ext::AnimationPlayer::new(),
            anim_name: "NewAnim".into(),
            anim_target: Some(mover.0),
            debug_breaks: vec![],
            debug_trace: vec![],
            debug_vars: vec![],
            drag_node: None,
            drag_axis: None,
            game_actions: false,
            game_prev: [false; 5],
            about_open: false,
            settings_open: false,
            settings_text: String::new(),
        };
        app.apply_theme_preset();
        app.load_editor_settings();
        app.sync_3d_camera();
        app
    }

    // ------------------------------------------------------------ helpers --
    fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 400 {
            self.log.drain(..self.log.len() - 400);
        }
    }

    fn snap(&self, v: f32) -> f32 {
        if self.snap_enabled && self.snap_step > 0.0 {
            (v / self.snap_step).round() * self.snap_step
        } else {
            v
        }
    }

    fn step_once(&mut self) {
        let expired = self.engine.tree.process_frame(1.0 / 60.0);
        for t in expired {
            self.log.push(format!("timer {t} finished"));
        }
        self.engine.audio.tick(1.0 / 60.0);
        self.sync_3d_camera();
        self.engine.rendering.render();
        self.engine.frames += 1;
        if let Some(id) = self.selected {
            for line in self.engine.tree.script_output(ObjectId(id)).to_vec() {
                let line = line;
                if self.log.last().map(|l| l != &line).unwrap_or(true) {
                    self.log(line);
                }
            }
            self.engine.tree.clear_script_output(ObjectId(id));
        }
    }

    fn apply_theme_preset(&self) {}

    fn load_editor_settings(&mut self) {
        let path = "user://editor_settings.json";
        if let Ok(s) = godot_host::ProjectSettings::load_json(path) {
            if let Some(v) = s.get("editor/theme_dark") {
                self.theme_dark = v.to_bool();
            }
            if let Some(v) = s.get("editor/snap_enabled") {
                self.snap_enabled = v.to_bool();
            }
            if let Some(v) = s.get("editor/snap_step") {
                self.snap_step = v.to_float() as f32;
            }
            if let Some(godot_core::Variant::String(p)) = s.get("editor/last_scene") {
                self.fs_dir = p.clone();
            }
        }
    }

    fn save_editor_settings(&mut self) {
        let mut s = godot_host::ProjectSettings::new();
        s.set("editor/theme_dark", Variant::Bool(self.theme_dark));
        s.set("editor/snap_enabled", Variant::Bool(self.snap_enabled));
        s.set("editor/snap_step", Variant::Float(self.snap_step as f64));
        s.set("editor/last_scene", Variant::String(self.fs_dir.clone()));
        if let Err(e) = s.save_json("user://editor_settings.json") {
            self.log.push(format!("settings save failed: {e}"));
        }
    }

    fn clear_scene(&mut self) {
        let kids = self.engine.tree.get_children(self.engine.tree.root_id()).unwrap_or_default();
        for k in kids {
            let _ = self.engine.tree.free(k);
        }
        self.selected = None;
        self.log("scene cleared".to_string());
    }

    fn save_scene_tscn(&mut self, path: &str) {
        match godot_scene::PackedScene::pack(&self.engine.tree, None) {
            Ok(packed) => {
                let json = packed.to_json();
                match godot_host::FileAccess::open_write(path) {
                    Ok(mut f) => {
                        f.write_text(&json);
                        if let Err(e) = f.close() {
                            self.log(format!("save failed: {e}"));
                        } else {
                            self.log(format!("saved {} ({} nodes)", path, self.engine.tree.node_count()));
                        }
                    }
                    Err(e) => self.log(format!("save failed: {e}")),
                }
            }
            Err(e) => self.log(format!("pack failed: {e}")),
        }
    }

    fn open_scene_tscn(&mut self, path: &str) {
        match godot_host::FileAccess::open_read(path) {
            Ok(mut f) => {
                let text = f.read_text();
                match godot_scene::PackedScene::from_json(&text) {
                    Ok(packed) => {
                        self.clear_scene();
                        let root = self.engine.tree.root_id();
                    match packed.instantiate(&mut self.engine.tree, Some(root)) {
                            Ok(id) => {
                                self.selected = Some(id.0);
                                self.log(format!("opened {path}"));
                            }
                            Err(e) => self.log(format!("instantiate failed: {e}")),
                        }
                    }
                    Err(e) => self.log(format!("parse failed: {e}")),
                }
            }
            Err(e) => self.log(format!("open failed: {e}")),
        }
    }

    // ---------------------------------------------------------- 3D camera --
    fn sync_3d_camera(&mut self) {
        // Find the first camera RID is overkill; camera id 1 exists in demo but
        // scenes vary — instead drive every camera-less viewport render through
        // the orbit rig by updating ALL cameras (editor owns them).
        let yaw = self.cam3.yaw;
        let pitch = self.cam3.pitch;
        let dist = self.cam3.dist.max(0.1);
        let dir = godot_core::Vector3::new(yaw.cos() * pitch.cos(), pitch.sin(), yaw.sin() * pitch.cos());
        let pos = self.cam3.target + dir * dist;
        let z = (pos - self.cam3.target).normalized();
        let up = godot_core::Vector3::UP;
        let x = up.cross(z).normalized();
        let x = if x.length_squared() < 1e-8 { godot_core::Vector3::RIGHT } else { x };
        let y = z.cross(x).normalized();
        let t = godot_core::Transform3D::new(godot_core::Basis { x, y, z }, pos);
        // Update every camera (editor viewports only).
        for i in 1..=8u64 {
            let rid = godot_core::Rid(i);
            if self.engine.rendering.camera_set_transform(rid, t) {
                if self.cam3.ortho {
                    self.engine.rendering.camera_set_ortho(rid, Some(self.cam3.ortho_size));
                } else {
                    self.engine.rendering.camera_set_ortho(rid, None);
                }
            }
        }
    }

    fn world_to_screen_3d(&self, p: godot_core::Vector3, view_px: egui::Vec2) -> Option<egui::Pos2> {
        // Rebuild view/proj exactly like the software renderer does.
        let yaw = self.cam3.yaw;
        let pitch = self.cam3.pitch;
        let dist = self.cam3.dist.max(0.1);
        let dir = godot_core::Vector3::new(yaw.cos() * pitch.cos(), pitch.sin(), yaw.sin() * pitch.cos());
        let pos = self.cam3.target + dir * dist;
        let z = (pos - self.cam3.target).normalized();
        let x = godot_core::Vector3::UP.cross(z).normalized();
        let x = if x.length_squared() < 1e-8 { godot_core::Vector3::RIGHT } else { x };
        let y = z.cross(x).normalized();
        let view = godot_core::Transform3D::new(godot_core::Basis { x, y, z }, pos).affine_inverse();
        let v = view.xform(p);
        if v.z > -0.01 {
            return None;
        }
        // Match renderer: 64x216? No — use viewport pixel dims for NDC mapping.
        if self.cam3.ortho {
            let s = self.cam3.ortho_size.max(0.01);
            let nx = v.x / (s * (VW as f32 / VH as f32));
            let ny = v.y / s;
            Some(egui::Pos2::new((nx * 0.5 + 0.5) * view_px.x, (0.5 - ny * 0.5) * view_px.y))
        } else {
            let aspect = VW as f32 / VH as f32;
            let proj = godot_core::Projection::create_perspective(70f32.to_radians(), aspect, 0.05, 100.0);
            let c = proj.xform(godot_core::Vector4::new(v.x, v.y, v.z, 1.0));
            if c.w.abs() < 1e-6 {
                return None;
            }
            let (nx, ny) = (c.x / c.w, c.y / c.w);
            Some(egui::Pos2::new((nx * 0.5 + 0.5) * view_px.x, (0.5 - ny * 0.5) * view_px.y))
        }
    }

    // ---------------------------------------------------------------- tree --
    fn show_tree(&mut self, ui: &mut egui::Ui) {
        let root = self.engine.tree.root_id();
        self.show_node(ui, root, 0);
    }

    fn show_node(&mut self, ui: &mut egui::Ui, id: ObjectId, depth: usize) {
        let (name, class, kids, me, has_script) = match self.engine.tree.get_stored(id) {
            Ok(n) => (
                n.name().to_owned(),
                n.class().to_owned(),
                n.data().children.clone(),
                n.id().0,
                self.engine.tree.script_of(n.id()).is_some(),
            ),
            Err(_) => return,
        };
        ui.horizontal(|ui| {
            let indent = "  ".repeat(depth.min(8));
            let badge = if has_script { "📜" } else { "" };
            let label = format!("{indent}{badge}{name}  [{class}]");
            let is_sel = self.selected == Some(me);
            if ui.selectable_label(is_sel, label).clicked() {
                self.selected = Some(me);
            }
        });
        for kid in kids {
            self.show_node(ui, kid, depth + 1);
        }
    }

    // ---------------------------------------------------------- inspector --
    fn show_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected else {
            ui.label("Nothing selected.");
            return;
        };
        let oid = ObjectId(id);
        let (name, class) = match self.engine.tree.get_stored(oid) {
            Ok(n) => (n.name().to_owned(), n.class().to_owned()),
            Err(_) => {
                ui.label("Node was freed.");
                return;
            }
        };
        ui.heading(format!("{name}"));
        ui.label(format!("class: {class}  id: #{id}"));
        ui.separator();
        ui.collapsing("Node", |ui| {
            let mut rename = name.clone();
            if ui.text_edit_singleline(&mut rename).changed() && !rename.trim().is_empty() {
                let _ = self.engine.tree.set_prop(oid, "name", Variant::String(rename));
            }
            if let Ok(Variant::Bool(v)) = self.engine.tree.get_prop(oid, "visible") {
                let mut vis = v;
                if ui.checkbox(&mut vis, "visible").changed() {
                    let _ = self.engine.tree.set_prop(oid, "visible", Variant::Bool(vis));
                }
            }
        });
        ui.collapsing("Transform", |ui| {
            if let Ok(Variant::Vector2(p)) = self.engine.tree.get_prop(oid, "position") {
                let (mut x, mut y) = (p.x, p.y);
                ui.horizontal(|ui| {
                    ui.label("position");
                    ui.add(egui::DragValue::new(&mut x).speed(1.0));
                    ui.add(egui::DragValue::new(&mut y).speed(1.0));
                });
                if x != p.x || y != p.y {
                    let (x, y) = (self.snap(x), self.snap(y));
                    let _ = self.engine.tree.set_prop(oid, "position", Variant::Vector2(godot_core::Vector2::new(x, y)));
                }
            }
            if let Ok(Variant::Vector3(p)) = self.engine.tree.get_prop(oid, "position") {
                let (mut x, mut y, mut z) = (p.x, p.y, p.z);
                ui.horizontal(|ui| {
                    ui.label("position");
                    ui.add(egui::DragValue::new(&mut x).speed(0.1));
                    ui.add(egui::DragValue::new(&mut y).speed(0.1));
                    ui.add(egui::DragValue::new(&mut z).speed(0.1));
                });
                if (x, y, z) != (p.x, p.y, p.z) {
                    let (x, y, z) = (self.snap(x), self.snap(y), self.snap(z));
                    let _ = self.engine.tree.set_prop(oid, "position", Variant::Vector3(godot_core::Vector3::new(x, y, z)));
                }
            }
            if let Ok(Variant::Vector3(r)) = self.engine.tree.get_prop(oid, "rotation") {
                let (mut x, mut y, mut z) = (r.x.to_degrees(), r.y.to_degrees(), r.z.to_degrees());
                ui.horizontal(|ui| {
                    ui.label("rotation°");
                    ui.add(egui::DragValue::new(&mut x).speed(1.0));
                    ui.add(egui::DragValue::new(&mut y).speed(1.0));
                    ui.add(egui::DragValue::new(&mut z).speed(1.0));
                });
                let nr = godot_core::Vector3::new(x.to_radians(), y.to_radians(), z.to_radians());
                if nr != r {
                    let _ = self.engine.tree.set_prop(oid, "rotation", Variant::Vector3(nr));
                }
            }
            if let Ok(v) = self.engine.tree.get_prop(oid, "scale") {
                ui.label(format!("scale: {}", v.to_string_lossy()));
            }
        });
        ui.collapsing("Groups", |ui| {
            if let Ok(n) = self.engine.tree.get_stored(oid) {
                let groups: Vec<String> = n.data().groups.iter().cloned().collect();
                ui.label(if groups.is_empty() { "(none)".into() } else { groups.join(", ") });
            }
        });
        ui.separator();
        if ui.button("Queue free").clicked() {
            let _ = self.engine.tree.queue_free(oid);
            self.log(format!("queued free #{id}"));
        }
    }

    fn show_node_tab(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected else {
            ui.label("Nothing selected.");
            return;
        };
        let class = self.engine.tree.get_stored(ObjectId(id)).map(|n| n.class().to_owned()).unwrap_or_default();
        ui.heading("Signals");
        let db = godot_core::global_db();
        match db.get(&class).or_else(|| db.get("Node")) {
            Some(def) if !def.signals.is_empty() => {
                for s in &def.signals {
                    ui.horizontal(|ui| {
                        ui.label(format!("⚡ {}({})", s.name, s.args.len()));
                        if ui.small_button("emit").clicked() {
                            self.log(format!("emit {} on #{id} (stub: no listeners in headless runtime)", s.name));
                        }
                    });
                }
            }
            _ => {
                ui.label("(no signals)");
            }
        }
        ui.separator();
        ui.heading("Methods");
        if let Some(def) = db.get(&class) {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for m in def.methods.iter().take(40) {
                    ui.monospace(format!("{}({})", m.name, m.args.len()));
                }
            });
        }
    }

    // ------------------------------------------------------------ viewport --
    fn show_viewport_tex(&mut self, ui: &mut egui::Ui, pixels: Vec<egui::Color32>, w: usize, h: usize, scale: f32, is_3d: bool) {
        let image = egui::ColorImage::new([w, h], pixels);
        let size = egui::vec2(w as f32 * scale, h as f32 * scale);
        if is_3d {
            match &mut self.tex {
                Some(tex) => {
                    tex.set(image, egui::TextureOptions::NEAREST);
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
                }
                None => {
                    let tex = ui.ctx().load_texture("viewport3d", image, egui::TextureOptions::NEAREST);
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
                    self.tex = Some(tex);
                }
            }
        } else {
            match &mut self.tex2d {
                Some(tex) => {
                    tex.set(image, egui::TextureOptions::NEAREST);
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
                }
                None => {
                    let tex = ui.ctx().load_texture("viewport2d", image, egui::TextureOptions::NEAREST);
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
                    self.tex2d = Some(tex);
                }
            }
        }
    }

    fn fb_to_pixels(fb: &[godot_core::Color]) -> Vec<egui::Color32> {
        fb.iter()
            .map(|c| {
                egui::Color32::from_rgba_unmultiplied(
                    (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                    (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                    (c.b.clamp(0.0, 1.0) * 255.0) as u8,
                    255,
                )
            })
            .collect()
    }

    /// GPU-resident 3D viewport: renders the scene with the window's own wgpu
    /// device into a registered texture — zero CPU readback, unlike the
    /// `show_viewport_tex` fallback used under the glow renderer.
    /// Returns true when the native path handled display.
    fn show_viewport_3d_native(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame, rect: egui::Rect) -> bool {
        let rs = match frame.wgpu_render_state() {
            Some(rs) => rs,
            None => return false,
        };
        let (w, h, clear, quads) = match self.engine.rendering.viewport_quads(self.vp) {
            Some(q) => q,
            None => return false,
        };
        if self.native.is_none() {
            let backend = godot_servers::wgpu_backend::WgpuBackend::with_device_queue(
                rs.device.clone(),
                rs.queue.clone(),
                rs.adapter.get_info().name.clone(),
            );
            self.native = Some(NativeView { backend, tex_id: None, w: 0, h: 0 });
            self.log("native wgpu viewport on: ".to_string() + &rs.adapter.get_info().name);
        }
        let nv = match self.native.as_mut() {
            Some(nv) => nv,
            None => return false,
        };
        if nv.tex_id.is_none() || nv.w != w || nv.h != h {
            if let Some(old) = nv.tex_id.take() {
                rs.renderer.write().free_texture(&old);
            }
            // Allocate the target, then expose it to egui as a native texture.
            nv.backend.render_quads_native(w, h, [0.0, 0.0, 0.0, 1.0], &[]);
            match nv.backend.target_view() {
                Some(view) => {
                    nv.tex_id = Some(rs.renderer.write().register_native_texture(
                        &rs.device,
                        view,
                        wgpu::FilterMode::Nearest,
                    ));
                    nv.w = w;
                    nv.h = h;
                }
                None => return false,
            }
        }
        nv.backend.render_quads_native(w, h, clear, &quads);
        if let Some(id) = nv.tex_id {
            ui.put(rect, egui::Image::new(egui::load::SizedTexture::new(id, rect.size())));
            true
        } else {
            false
        }
    }

    fn show_viewport_3d(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cam3.grid, "Grid");
            ui.checkbox(&mut self.cam3.ortho, "Ortho");
            if self.cam3.ortho {
                ui.add(egui::DragValue::new(&mut self.cam3.ortho_size).speed(0.1).range(0.5..=50.0));
            }
            ui.label("Snap:");
            ui.checkbox(&mut self.snap_enabled, "");
            ui.add(egui::DragValue::new(&mut self.snap_step).speed(0.1).range(0.01..=10.0));
        });
        // Camera input: MMB orbit, Shift/RMB pan, wheel zoom.
        let mut orbit = false;
        ui.horizontal(|ui| {
            ui.label("drag MMB: orbit · Shift+MMB/RMB: pan · wheel: zoom");
        });
        let avail = ui.available_size();
        let (resp, painter) = ui.allocate_painter(avail, egui::Sense::click_and_drag());
        // Wheel zoom.
        let wheel: f32 = ui.input(|i| i.events.iter().filter_map(|e| match e {
            egui::Event::MouseWheel { delta, .. } => Some(delta.y),
            _ => None,
        }).sum());
        if wheel != 0.0 && resp.hovered() {
            if self.cam3.ortho {
                self.cam3.ortho_size = (self.cam3.ortho_size * (1.0 - wheel * 0.001 * 60.0 / 60.0)).clamp(0.5, 50.0);
            } else {
                self.cam3.dist = (self.cam3.dist * (1.0 - wheel * 0.001)).clamp(0.5, 100.0);
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
            let shift = ui.input(|i| i.modifiers.shift);
            let d = resp.drag_delta();
            if resp.dragged_by(egui::PointerButton::Secondary) || shift {
                // Pan in camera plane.
                let s = self.cam3.dist * 0.0016;
                let right = godot_core::Vector3::new(self.cam3.yaw.cos(), 0.0, self.cam3.yaw.sin());
                let up2 = godot_core::Vector3::UP;
                self.cam3.target = self.cam3.target - right * d.x * s + up2 * d.y * s;
            } else {
                self.cam3.yaw -= d.x * 0.005;
                self.cam3.pitch = (self.cam3.pitch - d.y * 0.005).clamp(-1.45, 1.45);
                orbit = true;
            }
        }
        let _ = orbit;
        self.sync_3d_camera();
        // Texture: native GPU path first, CPU readback fallback (glow renderer).
        let rect = resp.rect;
        let native_ok = self.show_viewport_3d_native(ui, frame, rect);
        if !native_ok {
            self.engine.rendering.render();
            let fb = self.engine.rendering.capture(self.vp).unwrap_or_default();
            self.show_viewport_tex(ui, Self::fb_to_pixels(&fb), VW as usize, VH as usize, 1.4, true);
        }
        let backend_name: String = if native_ok {
            self.native.as_ref().map(|nv| nv.backend.adapter_name().to_owned()).unwrap_or_default()
        } else {
            "software readback".to_string()
        };
        // Overlay: grid + view cube + selection marker, drawn in screen space.
        let to_px = |p: egui::Pos2| p + rect.min.to_vec2();
        if self.cam3.grid {
            let stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(70));
            for i in -5..=5 {
                let a = godot_core::Vector3::new(i as f32, 0.0, -5.0);
                let b = godot_core::Vector3::new(i as f32, 0.0, 5.0);
                if let (Some(pa), Some(pb)) = (self.world_to_screen_3d(a, rect.size()), self.world_to_screen_3d(b, rect.size())) {
                    painter.line_segment([to_px(pa), to_px(pb)], stroke);
                }
                let a = godot_core::Vector3::new(-5.0, 0.0, i as f32);
                let b = godot_core::Vector3::new(5.0, 0.0, i as f32);
                if let (Some(pa), Some(pb)) = (self.world_to_screen_3d(a, rect.size()), self.world_to_screen_3d(b, rect.size())) {
                    painter.line_segment([to_px(pa), to_px(pb)], stroke);
                }
            }
        }
        // View cube (corner axes, X red / Y green / Z blue like Godot).
        {
            let c = to_px(egui::Pos2::new(rect.size().x - 46.0, 46.0));
            let yaw = self.cam3.yaw;
            let pitch = self.cam3.pitch;
            let dir = godot_core::Vector3::new(yaw.cos() * pitch.cos(), pitch.sin(), yaw.sin() * pitch.cos());
            let pos = dir * 10.0;
            let z = (pos).normalized();
            let x = godot_core::Vector3::UP.cross(z).normalized();
            let y = z.cross(x).normalized();
            let cols = [(x, egui::Color32::RED, "X"), (y, egui::Color32::GREEN, "Y"), (z, egui::Color32::from_rgb(80, 140, 255), "Z")];
            for (ax, col, tag) in cols {
                let end = c + egui::vec2(ax.x * 26.0, -ax.y * 26.0);
                painter.line_segment([c, end], egui::Stroke::new(2.0, col));
                painter.text(end, egui::Align2::CENTER_CENTER, tag, egui::FontId::monospace(11.0), col);
            }
        }
        // Selection marker.
        if let Some(id) = self.selected {
            if let Ok(p) = self.engine.tree.get_prop(ObjectId(id), "position") {
                let wp = match p {
                    Variant::Vector3(v) => Some(v),
                    Variant::Vector2(v) => Some(godot_core::Vector3::new(v.x, v.y, 0.0)),
                    _ => None,
                };
                if let Some(wp) = wp {
                    if let Some(sp) = self.world_to_screen_3d(wp, rect.size()) {
                        painter.circle_stroke(to_px(sp), 8.0, egui::Stroke::new(2.0, egui::Color32::YELLOW));
                    }
                }
            }
        }
        // Click-select nearest projected instance center.
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let local = pos - rect.min.to_vec2();
                let mut best: Option<(u64, f32)> = None;
                for (rid, aabb) in self.engine.rendering.instance_aabbs() {
                    let c = aabb.position + aabb.size * 0.5;
                    if let Some(sp) = self.world_to_screen_3d(c, rect.size()) {
                        let d = ((sp.x - local.x).powi(2) + (sp.y - local.y).powi(2)).sqrt();
                        if d < 24.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                            // Map instance -> node is unknown here; select by proximity is
                            // viewport-only feedback until instances carry node links.
                            let _ = rid;
                            best = Some((0, d));
                        }
                    }
                }
                if best.is_some() {
                    self.log("clicked a rendered instance (node link arrives with instance metadata)".to_string());
                }
            }
        }
        ui.label(format!(
            "draws={} tris={} frames={} cam yaw={:.1}° pitch={:.1}° d={:.1} [{}]",
            self.engine.rendering.stats.draw_calls,
            self.engine.rendering.stats.triangles,
            self.engine.rendering.stats.frames,
            self.cam3.yaw.to_degrees(),
            self.cam3.pitch.to_degrees(),
            self.cam3.dist,
            backend_name
        ));
    }

    fn show_viewport_2d(&mut self, ui: &mut egui::Ui) {
        // Hotkeys (not while typing).
        let typing = ui.ctx().egui_wants_keyboard_input();
        if !typing {
            ui.input(|i| {
                if i.key_pressed(egui::Key::Q) {
                    self.gizmo = GizmoMode::Select;
                }
                if i.key_pressed(egui::Key::W) {
                    self.gizmo = GizmoMode::Translate;
                }
                if i.key_pressed(egui::Key::E) {
                    self.gizmo = GizmoMode::Rotate;
                }
                if i.key_pressed(egui::Key::R) {
                    self.gizmo = GizmoMode::Scale;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Q Select  W Move  E Rotate  R Scale");
            ui.checkbox(&mut self.cam2.grid, "Grid");
            ui.label("Snap:");
            ui.checkbox(&mut self.snap_enabled, "");
            ui.add(egui::DragValue::new(&mut self.snap_step).speed(0.5).range(0.5..=64.0));
            ui.label(format!("zoom {:.0}%", self.cam2.zoom * 100.0));
        });
        let avail = ui.available_size();
        let (resp, painter) = ui.allocate_painter(avail, egui::Sense::click_and_drag());
        let rect = resp.rect;
        let mut center = self.cam2.center;
        let mut zoom = self.cam2.zoom;
        let w2s = |w: godot_core::Vector2, center: godot_core::Vector2, zoom: f32| {
            let rel = (w - center) * zoom;
            egui::Pos2::new(rect.center().x + rel.x, rect.center().y + rel.y)
        };
        let s2w = |p: egui::Pos2, center: godot_core::Vector2, zoom: f32| {
            let rel = egui::vec2(p.x - rect.center().x, p.y - rect.center().y) / zoom;
            godot_core::Vector2::new(center.x + rel.x, center.y + rel.y)
        };
        // Wheel zoom to cursor.
        let wheel: f32 = ui.input(|i| {
            i.events.iter().filter_map(|e| match e {
                egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                _ => None,
            }).sum()
        });
        if wheel != 0.0 && resp.hovered() {
            if let Some(mp) = resp.hover_pos() {
                let before = s2w(mp, center, zoom);
                zoom = (zoom * (1.0 + wheel * 0.001)).clamp(0.1, 8.0);
                let after = s2w(mp, center, zoom);
                center = center + (before - after);
            }
        }
        // Pan: MMB or Space+LMB.
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        if resp.dragged_by(egui::PointerButton::Middle)
            || (space && resp.dragged_by(egui::PointerButton::Primary))
        {
            let d = resp.drag_delta() / zoom;
            center = center - godot_core::Vector2::new(d.x, d.y);
        }
        // Grid.
        if self.cam2.grid {
            let step = 32.0 * zoom;
            if step > 6.0 {
                let stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(60));
                // Iterate screen lines aligned to world grid.
                let x0 = (s2w(egui::Pos2::new(rect.left(), 0.0), center, zoom).x / 32.0).floor() * 32.0;
                let x1 = s2w(egui::Pos2::new(rect.right(), 0.0), center, zoom).x;
                let mut gx = x0;
                while gx <= x1 {
                    let a = w2s(godot_core::Vector2::new(gx, s2w(egui::Pos2::new(0.0, rect.top()), center, zoom).y), center, zoom);
                    let b = w2s(godot_core::Vector2::new(gx, s2w(egui::Pos2::new(0.0, rect.bottom()), center, zoom).y), center, zoom);
                    painter.line_segment([a, b], stroke);
                    gx += 32.0;
                }
                let y0 = (s2w(egui::Pos2::new(0.0, rect.bottom()), center, zoom).y / 32.0).floor() * 32.0;
                let y1 = s2w(egui::Pos2::new(0.0, rect.top()), center, zoom).y;
                let mut gy = y0;
                while gy <= y1 {
                    let a = w2s(godot_core::Vector2::new(s2w(egui::Pos2::new(rect.left(), 0.0), center, zoom).x, gy), center, zoom);
                    let b = w2s(godot_core::Vector2::new(s2w(egui::Pos2::new(rect.right(), 0.0), center, zoom).x, gy), center, zoom);
                    painter.line_segment([a, b], stroke);
                    gy += 32.0;
                }
            }
        }
        self.cam2.center = center;
        self.cam2.zoom = zoom;
        // Nodes as labeled boxes (32px at zoom 1).
        let all = self.collect_nodes_2d();
        for (id, wpos, cname) in &all {
            let half = 16.0 * self.cam2.zoom;
            let c = w2s(*wpos, self.cam2.center, self.cam2.zoom);
            let r = egui::Rect::from_center_size(c, egui::vec2(half * 2.0, half * 2.0));
            let sel = self.selected == Some(*id);
            painter.rect_stroke(r, 2.0, egui::Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { egui::Color32::YELLOW } else { egui::Color32::from_rgb(120, 180, 240) }), egui::StrokeKind::Middle);
            painter.text(
                egui::Pos2::new(r.left(), r.top() - 4.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{cname}"),
                egui::FontId::monospace(11.0),
                egui::Color32::LIGHT_GRAY,
            );
        }
        // Gizmo for selected Node2D.
        if let Some(id) = self.selected {
            if let Ok(Variant::Vector2(p)) = self.engine.tree.get_prop(ObjectId(id), "position") {
                self.draw_gizmo_2d(&painter, &resp, p, id);
            }
        }
        // Click select (topmost).
        if resp.clicked_by(egui::PointerButton::Primary) && self.drag_node.is_none() {
            if let Some(mp) = resp.interact_pointer_pos() {
                let w = s2w(mp, self.cam2.center, self.cam2.zoom);
                let mut best: Option<(u64, f32)> = None;
                for (id, wpos, _) in &all {
                    let half = 16.0 + 6.0 / self.cam2.zoom;
                    let d = (*wpos - w).length();
                    if d < half && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                        best = Some((*id, d));
                    }
                }
                self.selected = best.map(|(id, _)| id);
            }
        }
        // End drag.
        if resp.drag_stopped() {
            self.drag_node = None;
            self.drag_axis = None;
        }
    }

    fn collect_nodes_2d(&self) -> Vec<(u64, godot_core::Vector2, String)> {
        let mut out = vec![];
        self.collect_2d_sub(self.engine.tree.root_id(), godot_core::Transform2D::IDENTITY, &mut out);
        out
    }

    fn collect_2d_sub(&self, id: ObjectId, parent_g: godot_core::Transform2D, out: &mut Vec<(u64, godot_core::Vector2, String)>) {
        let Ok(st) = self.engine.tree.get_stored(id) else { return };
        // Local transform of this node (identity for non-2D).
        let local = match st {
            godot_scene::StoredNode::N2D(n) => n.transform(),
            _ => godot_core::Transform2D::IDENTITY,
        };
        let global = parent_g * local;
        if matches!(st, godot_scene::StoredNode::N2D(_)) {
            out.push((st.id().0, global.origin, st.name().to_owned()));
        }
        for kid in st.data().children.clone() {
            self.collect_2d_sub(kid, global, out);
        }
    }

    fn draw_gizmo_2d(&mut self, painter: &egui::Painter, resp: &egui::Response, p: godot_core::Vector2, id: u64) {
        let rect = resp.rect;
        let w2s = |w: godot_core::Vector2| {
            let rel = (w - self.cam2.center) * self.cam2.zoom;
            egui::Pos2::new(rect.center().x + rel.x, rect.center().y + rel.y)
        };
        let s2w = |pt: egui::Pos2| {
            let rel = egui::vec2(pt.x - rect.center().x, pt.y - rect.center().y) / self.cam2.zoom;
            godot_core::Vector2::new(self.cam2.center.x + rel.x, self.cam2.center.y + rel.y)
        };
        let c = w2s(p);
        let len = 44.0;
        match self.gizmo {
            GizmoMode::Translate | GizmoMode::Select => {
                let xa = c + egui::vec2(len, 0.0);
                let ya = c + egui::vec2(0.0, len);
                painter.line_segment([c, xa], egui::Stroke::new(2.0, egui::Color32::RED));
                painter.line_segment([c, ya], egui::Stroke::new(2.0, egui::Color32::GREEN));
                painter.circle_filled(xa, 4.0, egui::Color32::RED);
                painter.circle_filled(ya, 4.0, egui::Color32::GREEN);
                // Drag handling.
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(mp) = resp.interact_pointer_pos() {
                        self.drag_node = Some(id);
                        self.drag_axis = if (mp - xa).length() < 10.0 {
                            Some('x')
                        } else if (mp - ya).length() < 10.0 {
                            Some('y')
                        } else if (mp - c).length() < 12.0 {
                            Some('f')
                        } else {
                            None
                        };
                        if self.drag_axis.is_none() {
                            self.drag_node = None;
                        }
                    }
                }
                if self.drag_node == Some(id) {
                    if let Some(mp) = resp.interact_pointer_pos() {
                        let w = s2w(mp);
                        let mut np = p;
                        match self.drag_axis {
                            Some('x') => np.x = self.snap(w.x),
                            Some('y') => np.y = self.snap(w.y),
                            _ => {
                                np.x = self.snap(w.x);
                                np.y = self.snap(w.y);
                            }
                        }
                        if np != p {
                            let _ = self.engine.tree.set_prop(ObjectId(id), "position", Variant::Vector2(np));
                        }
                    }
                }
            }
            GizmoMode::Rotate => {
                painter.circle_stroke(c, 30.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(120, 200, 255)));
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(mp) = resp.interact_pointer_pos() {
                        if let Ok(Variant::Float(rot)) = self.engine.tree.get_prop(ObjectId(id), "rotation") {
                            let d = resp.drag_delta();
                            let nr = rot as f32 + d.x * 0.01;
                            let _ = self.engine.tree.set_prop(ObjectId(id), "rotation", Variant::Float(nr as f64));
                        } else {
                            let _ = self.engine.tree.set_prop(ObjectId(id), "rotation", Variant::Float(0.0));
                        }
                        let _ = mp;
                    }
                }
            }
            GizmoMode::Scale => {
                let h = c + egui::vec2(len, len);
                painter.line_segment([c, h], egui::Stroke::new(2.0, egui::Color32::YELLOW));
                painter.rect_filled(egui::Rect::from_center_size(h, egui::vec2(8.0, 8.0)), 1.0, egui::Color32::YELLOW);
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(mp) = resp.interact_pointer_pos() {
                        let w = s2w(mp);
                        let f = ((w - p).length() / 32.0).max(0.1);
                        let _ = self.engine.tree.set_prop(ObjectId(id), "scale", Variant::Vector2(godot_core::Vector2::new(f, f)));
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ UI shell --
    fn apply_theme(&self, ctx: &egui::Context) {
        if self.theme_dark {
            let mut v = egui::Visuals::dark();
            v.window_fill = egui::Color32::from_rgb(45, 47, 51);
            v.panel_fill = egui::Color32::from_rgb(56, 60, 64);
            v.faint_bg_color = egui::Color32::from_rgb(62, 66, 71);
            v.extreme_bg_color = egui::Color32::from_rgb(30, 31, 34);
            v.selection.bg_fill = egui::Color32::from_rgb(77, 157, 224);
            v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(56, 60, 64);
            ctx.set_visuals(v);
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }
    }

    fn show_menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Scene", |ui| {
                if ui.button("New Scene  (clears tree)").clicked() {
                    self.clear_scene();
                }
                if ui.button("Open res://scene.tscn.json").clicked() {
                    self.open_scene_tscn("res://scene.tscn.json");
                }
                if ui.button("Save res://scene.tscn.json  (Ctrl+S)").clicked() {
                    self.save_scene_tscn("res://scene.tscn.json");
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Project", |ui| {
                if ui.button("Project Settings…").clicked() {
                    self.settings_open = true;
                    self.settings_text = format!("application/name = {}\nphysics ticks = {}\nviewport = {:?}\n",
                        self.engine.settings.get("application/name").map(|v| v.to_string_lossy()).unwrap_or_default(),
                        self.engine.settings.physics_ticks(),
                        self.engine.settings.viewport_size());
                }
                if ui.button("Reload Scripts").clicked() {
                    self.log("scripts reload on next frame (attach again to refresh)".to_string());
                }
            });
            ui.menu_button("Debug", |ui| {
                if ui.button("Step Frame  (F7)").clicked() {
                    self.step_once();
                }
                if ui.button("Run Selected _process Once (traced)").clicked() {
                    self.debug_run_once();
                }
            });
            ui.menu_button("Editor", |ui| {
                ui.checkbox(&mut self.theme_dark, "Dark theme");
                ui.checkbox(&mut self.snap_enabled, "Use snap");
                ui.add(egui::DragValue::new(&mut self.snap_step).speed(0.5).range(0.5..=64.0));
                if ui.button("Save Editor Settings").clicked() {
                    self.save_editor_settings();
                    self.log("editor settings saved to user://editor_settings.json".to_string());
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About godot_rs").clicked() {
                    self.about_open = true;
                }
            });
        });
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Main screens, Godot-style, centered-ish.
            for (label, scr) in [("2D", MainScreen::Scene2D), ("3D", MainScreen::Scene3D),
                ("Script", MainScreen::Script), ("Game", MainScreen::Game), ("AssetLib", MainScreen::AssetLib)] {
                if ui.selectable_label(self.screen == scr, label).clicked() {
                    self.screen = scr;
                }
            }
            ui.separator();
            let label = if self.playing { "⏸ Stop (F8)" } else { "▶ Play (F5)" };
            if ui.button(label).clicked() {
                self.playing = !self.playing;
            }
            if ui.button("⏸ Pause").clicked() {
                self.playing = false;
            }
            if ui.button("Step ⏭ (F7)").clicked() {
                self.step_once();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{} frames · {} nodes · {} classes", self.engine.frames, self.engine.tree.node_count(), godot_core::global_db().len()));
            });
        });
        // Global hotkeys (not while typing).
        if !ui.ctx().egui_wants_keyboard_input() {
            let mut toggle: Option<MainScreen> = None;
            ui.input(|i| {
                if i.key_pressed(egui::Key::F5) {
                    self.playing = !self.playing;
                }
                if i.key_pressed(egui::Key::F8) {
                    self.playing = false;
                }
                if i.key_pressed(egui::Key::F7) {
                    self.step_once();
                }
                if i.modifiers.ctrl && i.key_pressed(egui::Key::S) {
                    self.save_scene_tscn("res://scene.tscn.json");
                }
                if i.key_pressed(egui::Key::F1) {
                    toggle = Some(MainScreen::Scene2D);
                }
                if i.key_pressed(egui::Key::F2) {
                    toggle = Some(MainScreen::Scene3D);
                }
                if i.key_pressed(egui::Key::F3) {
                    toggle = Some(MainScreen::Script);
                }
                if i.key_pressed(egui::Key::F4) {
                    toggle = Some(MainScreen::Game);
                }
            });
            if let Some(s) = toggle {
                self.screen = s;
            }
        }
    }

    fn show_bottom_dock(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (label, tab) in [("Output", BottomTab::Output), ("Debugger", BottomTab::Debugger), ("Animation", BottomTab::Animation)] {
                if ui.selectable_label(self.bottom == tab, label).clicked() {
                    self.bottom = tab;
                }
            }
            if self.bottom == BottomTab::Output && ui.small_button("Clear").clicked() {
                self.log.clear();
            }
        });
        match self.bottom {
            BottomTab::Output => self.show_output_tab(ui),
            BottomTab::Debugger => self.show_debugger_tab(ui),
            BottomTab::Animation => self.show_animation_tab(ui),
        }
    }

    fn show_output_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Output / Iris");
            if ui.button("Check").clicked() {
                match godot_script_iris::parse(&self.script_text) {
                    Ok(f) => {
                        let d = godot_script_iris::check(&f);
                        self.log(format!("check ok: {} props, {} fns, {} diagnostics", f.props.len(), f.fns.len(), d.len()));
                        for diag in d {
                            self.log(format!("{}: {}", diag.level, diag.message));
                        }
                    }
                    Err(e) => self.log(format!("parse error: {e}")),
                }
            }
            if ui.button("Attach to selected").clicked() {
                match self.selected {
                    Some(id) => match self.engine.tree.attach_script(ObjectId(id), self.script_text.clone()) {
                        Ok(()) => {
                            let _ = self.engine.tree.enable_process(ObjectId(id), true, false);
                            self.log(format!("script attached to #{id}"));
                        }
                        Err(e) => self.log(format!("attach failed: {e}")),
                    },
                    None => self.log("select a node first".to_string()),
                }
            }
            if ui.button("Import .gd").clicked() {
                match godot_script_iris::gdtranspile::transpile(&self.script_text) {
                    Ok(f) => {
                        let doc = godot_script_iris::doc_json(&f);
                        self.script_text = godot_script_iris::gdtranspile::emit_iris(&f);
                        self.log(format!("transpiled GDScript -> Iris: {} props, {} fns", doc.props.len(), doc.fns.len()));
                    }
                    Err(e) => self.log(format!("transpile failed (not GDScript?): {e}")),
                }
            }
        });
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.script_text).desired_rows(8).code_editor().desired_width(f32::INFINITY));
            });
            egui::ScrollArea::vertical().stick_to_bottom(true).show(&mut cols[1], |ui| {
                for line in self.log.clone() {
                    ui.monospace(line);
                }
            });
        });
    }

    fn show_debugger_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Debugger");
            if ui.button("Run selected _process once (traced)").clicked() {
                self.debug_run_once();
            }
            if ui.button("Clear trace").clicked() {
                self.debug_trace.clear();
                self.debug_vars.clear();
            }
        });
        ui.label("Breakpoints (function names, manual runs only):");
        let mut bps = std::mem::take(&mut self.debug_breaks);
        let mut new_bp = String::new();
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut new_bp);
            if ui.small_button("Add").clicked() && !new_bp.trim().is_empty() {
                bps.push(new_bp.trim().to_owned());
            }
        });
        bps.retain(|b| {
            let mut keep = true;
            ui.horizontal(|ui| {
                ui.monospace(format!("⏺ {b}"));
                if ui.small_button("✖").clicked() {
                    keep = false;
                }
            });
            keep
        });
        self.debug_breaks = bps;
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for line in self.debug_trace.clone() {
                ui.monospace(line);
            }
            if !self.debug_vars.is_empty() {
                ui.separator();
                ui.strong("Locals after run:");
                for (k, v) in self.debug_vars.clone() {
                    ui.monospace(format!("{k} = {v}"));
                }
            }
        });
    }

    fn debug_run_once(&mut self) {
        let Some(id) = self.selected else {
            self.log("select a node first".to_string());
            return;
        };
        let Some(src) = self.engine.tree.script_of(ObjectId(id)).map(|s| s.to_owned()) else {
            self.log("selected node has no attached script".to_string());
            return;
        };
        let file = match godot_script_iris::parse(&src) {
            Ok(f) => f,
            Err(e) => {
                self.log(format!("parse error: {e}"));
                return;
            }
        };
        let Some(func) = file.fns.iter().find(|f| f.name == "_process") else {
            self.log("no _process in script".to_string());
            return;
        };
        if self.debug_breaks.iter().any(|b| b == "_process") {
            self.debug_trace.push("break _process (manual run skipped body)".to_string());
            self.bottom = BottomTab::Debugger;
            return;
        }
        let mut interp = godot_script_iris::Interpreter::new();
        interp.vars.insert("dt".to_owned(), godot_script_iris::Value::Float(1.0 / 60.0));
        if let Ok(n) = self.engine.tree.get_stored(ObjectId(id)) {
            interp.vars.insert("name".to_owned(), godot_script_iris::Value::Str(n.name().to_owned()));
            for key in ["position", "rotation", "scale", "visible"] {
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
        interp.run_traced("_process", &func.body);
        self.debug_trace.extend(interp.trace.clone());
        self.debug_trace.extend(interp.output.clone());
        self.debug_vars = interp.vars.iter().map(|(k, v)| (k.clone(), v.to_display())).collect();
        self.debug_vars.sort();
        self.bottom = BottomTab::Debugger;
    }

    fn show_animation_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Animation");
            ui.text_edit_singleline(&mut self.anim_name);
            if ui.button("New").clicked() && !self.anim_name.trim().is_empty() {
                let name = self.anim_name.trim().to_owned();
                self.anims.entry(name.clone()).or_insert_with(|| godot_scene_ext::Animation::new(&name, 2.0));
                self.log(format!("animation `{name}` ready"));
            }
        });
        let names: Vec<String> = self.anims.keys().cloned().collect();
        ui.horizontal(|ui| {
            ui.label("clip:");
            for n in &names {
                let cur = self.anim_player.current().as_deref() == Some(n.as_str());
                if ui.selectable_label(cur, n).clicked() {
                    if self.anim_player.play(n, 1.0).is_ok() {
                        self.log(format!("playing `{n}`"));
                    }
                }
            }
            if ui.button(if self.anim_player.is_playing() { "⏸" } else { "▶" }).clicked() {
                if self.anim_player.is_playing() {
                    self.anim_player.stop();
                } else if let Some(n) = names.first() {
                    let _ = self.anim_player.play(n, 1.0);
                }
            }
            if ui.button("Add key (selected position @ t)").clicked() {
                self.anim_add_key();
            }
        });
        // Scrub + sample apply.
        let len = self.current_anim_len();
        let mut t = self.anim_player.position().min(len);
        if ui.add(egui::Slider::new(&mut t, 0.0..=len.max(0.01)).text("t")).changed() {
            self.apply_anim_sample(t);
        }
        ui.label(format!("t={:.2}/{:.2} target=#{:?}", t, len, self.anim_target));
    }

    fn current_anim_len(&self) -> f64 {
        self.anim_player.current().as_ref().and_then(|n| self.anims.get(n)).map(|a| a.length).unwrap_or(2.0)
    }

    fn anim_add_key(&mut self) {
        let Some(id) = self.selected.or(self.anim_target) else {
            self.log("select a node first".to_string());
            return;
        };
        let name = if self.anim_name.trim().is_empty() { "NewAnim".to_string() } else { self.anim_name.trim().to_owned() };
        let anim = self.anims.entry(name.clone()).or_insert_with(|| godot_scene_ext::Animation::new(&name, 2.0));
        let t = self.anim_player.position().min(anim.length);
        let track = if let Some(i) = anim.tracks.iter().position(|tr| tr.path == "position") { i } else { anim.add_track("position") };
        match self.engine.tree.get_prop(ObjectId(id), "position") {
            Ok(v) => {
                if anim.track_add_key(track, t, v).is_ok() {
                    self.anim_target = Some(id);
                    self.log(format!("key added to `{name}` @ t={t:.2}"));
                }
            }
            Err(_) => self.log("selected node has no position".to_string()),
        }
    }

    fn apply_anim_sample(&mut self, t: f64) {
        let Some(cur) = self.anim_player.current() else { return };
        let samples: Vec<(String, godot_core::Variant)> = self
            .anims
            .get(&cur)
            .map(|a| {
                a.tracks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, tr)| a.sample(i, t).map(|v| (tr.path.clone(), v)))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(id) = self.anim_target {
            for (path, v) in samples {
                let prop = path.split(':').next().unwrap_or(&path);
                let _ = self.engine.tree.set_prop(ObjectId(id), prop, v);
            }
        }
    }

    fn show_script_screen(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Script — Iris (GDScript successor)");
            if ui.button("Check").clicked() {
                match godot_script_iris::parse(&self.script_text) {
                    Ok(f) => {
                        let d = godot_script_iris::check(&f);
                        self.log(format!("check ok: {} props, {} fns, {} diagnostics", f.props.len(), f.fns.len(), d.len()));
                    }
                    Err(e) => self.log(format!("parse error: {e}")),
                }
            }
            if ui.button("Attach to selected + Play").clicked() {
                match self.selected {
                    Some(id) => match self.engine.tree.attach_script(ObjectId(id), self.script_text.clone()) {
                        Ok(()) => {
                            let _ = self.engine.tree.enable_process(ObjectId(id), true, false);
                            self.playing = true;
                            self.log(format!("script attached to #{id}, playing"));
                        }
                        Err(e) => self.log(format!("attach failed: {e}")),
                    },
                    None => self.log("select a node first".to_string()),
                }
            }
            if ui.button("Import .gd (transpile)").clicked() {
                match godot_script_iris::gdtranspile::transpile(&self.script_text) {
                    Ok(f) => {
                        self.script_text = godot_script_iris::gdtranspile::emit_iris(&f);
                        self.log("transpiled GDScript -> Iris (see docs/GDSCRIPT_COVERAGE.md for scope)".to_string());
                    }
                    Err(e) => self.log(format!("transpile: {e}")),
                }
            }
        });
        if let Ok(f) = godot_script_iris::parse(&self.script_text) {
            let doc = godot_script_iris::doc_json(&f);
            ui.monospace(format!("{} : {} — {} props, {} fns", doc.node_class, doc.base_class, doc.props.len(), doc.fns.len()));
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut self.script_text).code_editor().desired_width(f32::INFINITY));
        });
    }

    fn show_game_screen(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.game_actions {
            for (a, keys) in [("ui_up", &['w', 'W'][..]), ("ui_down", &['s', 'S'][..]), ("ui_left", &['a', 'A'][..]), ("ui_right", &['d', 'D'][..]), ("ui_accept", &[' '][..])] {
                if !self.engine.input.has_action(a) {
                    self.engine.input.add_action(a, keys);
                }
            }
            self.game_actions = true;
        }
        // WASD + arrows + Space -> ui_* actions (arrows alias WASD/Space).
        let downs = ui.input(|i| {
            [
                i.key_down(egui::Key::W) || i.key_down(egui::Key::ArrowUp),
                i.key_down(egui::Key::S) || i.key_down(egui::Key::ArrowDown),
                i.key_down(egui::Key::A) || i.key_down(egui::Key::ArrowLeft),
                i.key_down(egui::Key::D) || i.key_down(egui::Key::ArrowRight),
                i.key_down(egui::Key::Space) || i.key_down(egui::Key::Enter),
            ]
        });
        let chars = ['w', 's', 'a', 'd', ' '];
        let actions = ["ui_up", "ui_down", "ui_left", "ui_right", "ui_accept"];
        for (((down, prev), ch), _act) in downs.iter().zip(self.game_prev.iter_mut()).zip(chars.iter()).zip(actions.iter()) {
            if *down && !*prev {
                self.engine.input.inject(godot_host::InputEvent::Key { key: godot_host::Key::Key(*ch), pressed: true, echo: false });
            } else if !*down && *prev {
                self.engine.input.inject(godot_host::InputEvent::Key { key: godot_host::Key::Key(*ch), pressed: false, echo: false });
            }
            *prev = *down;
        }
        ui.horizontal(|ui| {
            ui.label("Game — click viewport, WASD/arrows + Space drive ui_* actions:");
            for (act, down) in actions.iter().zip(downs.iter()) {
                ui.label(format!("{}:{}", act, if *down { "▼" } else { "·" }));
            }
        });
        let avail = ui.available_size();
        let (resp, _painter) = ui.allocate_painter(avail, egui::Sense::click());
        let mut drew_native = false;
        if frame.wgpu_render_state().is_some() {
            drew_native = self.show_viewport_3d_native(ui, frame, resp.rect);
        }
        if !drew_native {
            self.engine.rendering.render();
            self.show_viewport_tex(
                ui,
                Self::fb_to_pixels(&self.engine.rendering.capture(self.vp).unwrap_or_default()),
                VW as usize,
                VH as usize,
                1.8,
                true,
            );
        }
        if self.playing {
            ui.ctx().request_repaint();
        }
    }

    fn show_assetlib(&mut self, ui: &mut egui::Ui) {
        ui.heading("AssetLib");
        ui.label("Online asset library is out of scope for the offline port.");
        ui.label("Import local assets instead:");
        ui.horizontal(|ui| {
            if ui.button("Import glTF (.gltf JSON)").clicked() {
                match godot_host::FileAccess::open_read("res://model.gltf") {
                    Ok(mut f) => {
                        let text = f.read_text();
                        match godot_io::GltfDocument::from_json(&text) {
                            Ok(doc) => self.log(format!("glTF: {} meshes, {} nodes", doc.meshes.len(), doc.nodes.len())),
                            Err(e) => self.log(format!("glTF import failed: {e}")),
                        }
                    }
                    Err(_) => self.log("place a file at res://model.gltf to import".to_string()),
                }
            }
            if ui.button("Transpile demo.gd").clicked() {
                match godot_host::FileAccess::open_read("res://demo.gd") {
                    Ok(mut f) => {
                        let text = f.read_text();
                        match godot_script_iris::gdtranspile::transpile(&text) {
                            Ok(gd) => {
                                self.script_text = godot_script_iris::gdtranspile::emit_iris(&gd);
                                self.screen = MainScreen::Script;
                                self.log("transpiled res://demo.gd into the Script editor".to_string());
                            }
                            Err(e) => self.log(format!("transpile failed: {e}")),
                        }
                    }
                    Err(_) => self.log("place a file at res://demo.gd to transpile".to_string()),
                }
            }
        });
    }

    fn show_create_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.create_open;
        egui::Window::new("Create Node (Ctrl+A)").open(&mut open).show(ctx, |ui| {
            ui.text_edit_singleline(&mut self.create_search);
            let q = self.create_search.to_lowercase();
            let db = godot_core::global_db();
            let mut names: Vec<String> = db.class_names().into_iter().filter(|n| db.is_parent_class(n, "Node")).collect();
            names.sort();
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for n in names.into_iter().filter(|n| n.to_lowercase().contains(&q)) {
                    if ui.selectable_label(false, &n).double_clicked() {
                        self.create_node_of_class(&n);
                        self.create_open = false;
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Close").clicked() {
                    self.create_open = false;
                }
            });
        });
        self.create_open = open;
    }

    fn create_node_of_class(&mut self, class: &str) {
        let parent = self.selected.map(ObjectId).or(Some(self.engine.tree.root_id()));
        let res = match class {
            "Node2D" => self.engine.tree.add_2d(godot_scene::Node2D::new(class), parent).map(|id| id.0),
            "Node3D" => self.engine.tree.add_3d(godot_scene::Node3D::new(class), parent).map(|id| id.0),
            _ => self.engine.tree.add_base(godot_scene::Node::new(class), parent).map(|id| id.0),
        };
        match res {
            Ok(id) => {
                self.selected = Some(id);
                self.log(format!("created {class} #{id}"));
            }
            Err(e) => self.log(format!("create failed: {e}")),
        }
    }

    fn show_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Project Settings").open(&mut open).show(ctx, |ui| {
            let mut name = self.engine.settings.get("application/name").map(|v| v.to_string_lossy()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("application/name");
                if ui.text_edit_singleline(&mut name).changed() {
                    self.engine.settings.set("application/name", Variant::String(name.clone()));
                    self.project_name = name;
                }
            });
            ui.label(format!("viewport: {:?}", self.engine.settings.viewport_size()));
            ui.label(format!("physics ticks: {}", self.engine.settings.physics_ticks()));
            if ui.button("Save project.godot.json").clicked() {
                match self.engine.settings.save_json("res://project.godot.json") {
                    Ok(()) => self.log("saved res://project.godot.json".to_string()),
                    Err(e) => self.log(format!("save failed: {e}")),
                }
            }
        });
        self.settings_open = open;
    }

    fn show_about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about_open;
        egui::Window::new("About").open(&mut open).show(ctx, |ui| {
            ui.heading(format!("godot_rs {}", godot_core::ENGINE_VERSION));
            ui.label("Rust port of the Godot engine (headless core + software renderer).");
            ui.label(format!("{} · {} nodes loaded", std::env::consts::OS, self.engine.tree.node_count()));
        });
        self.about_open = open;
    }
}

fn parent_dir(path: &str) -> String {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(i) if i > 0 => t[..i].to_owned(),
        _ => "res://".into(),
    }
}

fn join_dir(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.apply_theme(ui.ctx());
        egui::Panel::top("menu").show(ui, |ui| self.show_menu(ui));
        egui::Panel::top("mainbar").show(ui, |ui| self.show_toolbar(ui));

        egui::Panel::left("left").resizable(true).default_size(260.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Scene");
            });
            egui::ScrollArea::vertical().max_height(ui.available_height() * 0.52).show(ui, |ui| {
                self.show_tree(ui)
            });
            ui.horizontal(|ui| {
                if ui.button("Create Node (Ctrl+A)").clicked() {
                    self.create_open = true;
                    self.create_search.clear();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("FileSystem");
                if ui.small_button("↑").clicked() {
                    self.fs_dir = parent_dir(&self.fs_dir);
                }
            });
            ui.monospace(self.fs_dir.clone());
            let entries = godot_host::DirAccess::list_dir(&self.fs_dir).unwrap_or_default();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in entries {
                    let full = join_dir(&self.fs_dir, &e);
                    let is_dir = godot_host::DirAccess::exists(&full);
                    let label = if is_dir { format!("📁 {e}") } else { format!("📄 {e}") };
                    if ui.selectable_label(false, label).double_clicked() {
                        if is_dir {
                            self.fs_dir = full;
                        } else if e.ends_with(".iris") {
                            match godot_host::FileAccess::open_read(&full) {
                                Ok(mut f) => {
                                    self.script_text = f.read_text();
                                    self.screen = MainScreen::Script;
                                    self.log(format!("loaded {full}"));
                                }
                                Err(err) => self.log(format!("open failed: {err}")),
                            }
                        } else if e.ends_with(".tscn.json") {
                            self.open_scene_tscn(&full);
                        } else {
                            self.log(format!("no editor for {e} (open externally)"));
                        }
                    }
                }
            });
        });

        egui::Panel::right("inspector").resizable(true).default_size(300.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Inspector");
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.show_inspector(ui);
                ui.separator();
                ui.collapsing("Node", |ui| self.show_node_tab(ui));
            });
        });

        egui::Panel::bottom("bottom").resizable(true).default_size(190.0).show(ui, |ui| {
            self.show_bottom_dock(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} frames", self.engine.frames));
                ui.label(format!("{} nodes", self.engine.tree.node_count()));
                ui.label(format!(
                    "draws={} tris={}",
                    self.engine.rendering.stats.draw_calls, self.engine.rendering.stats.triangles
                ));
                if !self.status.is_empty() {
                    ui.label(format!("— {}", self.status));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(if self.playing { "▶ running" } else { "⏸ stopped" });
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| match self.screen {
            MainScreen::Scene2D => {
                ui.heading("2D");
                self.show_viewport_2d(ui);
            }
            MainScreen::Scene3D => {
                ui.heading("3D");
                self.show_viewport_3d(ui, frame);
                self.status = format!("audio {:.1}s", self.engine.audio.time);
            }
            MainScreen::Script => self.show_script_screen(ui),
            MainScreen::Game => {
                ui.heading("Game");
                self.show_game_screen(ui, frame);
            }
            MainScreen::AssetLib => self.show_assetlib(ui),
        });

        if self.create_open {
            self.show_create_dialog(ui.ctx());
        }
        if self.settings_open {
            self.show_settings_window(ui.ctx());
        }
        if self.about_open {
            self.show_about_window(ui.ctx());
        }

        if self.playing {
            self.step_once();
            // Animation dock playback.
            if self.anim_player.is_playing() {
                let samples = self.anim_player.advance(1.0 / 60.0);
                if let Some(id) = self.anim_target {
                    for (path, v) in samples {
                        let prop = path.split(':').next().unwrap_or(&path).to_string();
                        let _ = self.engine.tree.set_prop(ObjectId(id), &prop, v);
                    }
                }
            }
            ui.ctx().request_repaint();
        }
    }
}
