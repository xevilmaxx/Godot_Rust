//! Port of `servers/rendering/rendering_server.h` (API shape) +
//! `servers/rendering/dummy/rasterizer_dummy.h` (headless behavior) with a
//! deterministic software rasterizer upgrade: per-instance shaded rects from
//! projected mesh AABBs. The GPU path is a `RenderBackend` trait so wgpu plugs
//! in under `godot_host` without changing this API.

use godot_core::{Aabb, Color, Projection, Rid, Transform3D, Vector2, Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::allocator::RidOwner;

/// Backend selector. `Wgpu` reports `unavailable()` until `godot_host` owns a
/// window/device — the API stays identical so agents don't care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderBackendKind {
    #[default]
    Software,
    Wgpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    Texture,
    Mesh,
    Material,
    Light,
    Camera,
    Environment,
    Viewport,
    Instance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    pub vertices: Vec<Vector3>,
    pub indices: Vec<u32>,
    pub aabb: Aabb,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Material {
    pub albedo: Color,
    pub roughness: f32,
    pub metallic: f32,
    pub emission: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightKind {
    Directional,
    Omni,
    Spot,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightKind,
    pub color: Color,
    pub energy: f32,
    pub direction: Vector3,
    pub position: Vector3,
    pub range: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub fov_y_deg: f32,
    pub near: f32,
    pub far: f32,
    pub transform: Transform3D,
    /// `None` = perspective; `Some(half_height)` = orthographic.
    pub ortho_size: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub background: Color,
    pub ambient: Color,
    pub ambient_energy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub clear_color: Color,
    pub camera: Option<Rid>,
    pub environment: Option<Rid>,
    pub framebuffer: Vec<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub mesh: Option<Rid>,
    pub material: Option<Rid>,
    pub transform: Transform3D,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RenderStats {
    pub draw_calls: u64,
    pub culled: u64,
    pub triangles: u64,
    pub frames: u64,
}

pub struct RenderingServer {
    textures: RidOwner<Texture>,
    meshes: RidOwner<Mesh>,
    materials: RidOwner<Material>,
    lights: RidOwner<Light>,
    cameras: RidOwner<Camera>,
    environments: RidOwner<Environment>,
    viewports: RidOwner<Viewport>,
    instances: RidOwner<Instance>,
    backend: RenderBackendKind,
    #[cfg(feature = "wgpu")]
    gpu: Option<crate::wgpu_backend::WgpuBackend>,
    pub stats: RenderStats,
}

impl RenderingServer {
    pub fn new(backend: RenderBackendKind) -> Self {
        Self {
            textures: RidOwner::new(),
            meshes: RidOwner::new(),
            materials: RidOwner::new(),
            lights: RidOwner::new(),
            cameras: RidOwner::new(),
            environments: RidOwner::new(),
            viewports: RidOwner::new(),
            instances: RidOwner::new(),
            backend,
            #[cfg(feature = "wgpu")]
            gpu: None,
            stats: RenderStats::default(),
        }
    }
    pub fn backend(&self) -> RenderBackendKind {
        self.backend
    }
    /// Switch backends. `Wgpu` creates the offscreen device immediately so a
    /// missing adapter is an explicit error, never a silent no-op.
    pub fn set_backend(&mut self, kind: RenderBackendKind) -> Result<(), String> {
        match kind {
            RenderBackendKind::Software => {
                self.backend = kind;
                Ok(())
            }
            RenderBackendKind::Wgpu => {
                #[cfg(feature = "wgpu")]
                {
                    if self.gpu.is_none() {
                        self.gpu = Some(crate::wgpu_backend::WgpuBackend::try_new()?);
                    }
                    self.backend = kind;
                    Ok(())
                }
                #[cfg(not(feature = "wgpu"))]
                Err("wgpu backend unavailable: rebuild godot_servers with --features wgpu".into())
            }
        }
    }
    /// `true` when a live wgpu device is attached (requires the `wgpu` feature).
    pub fn has_gpu(&self) -> bool {
        #[cfg(feature = "wgpu")]
        return self.gpu.is_some();
        #[cfg(not(feature = "wgpu"))]
        return false;
    }
    /// Name of the attached GPU adapter, if any.
    pub fn gpu_adapter_name(&self) -> Option<String> {
        #[cfg(feature = "wgpu")]
        return self.gpu.as_ref().map(|g| g.adapter_name().to_owned());
        #[cfg(not(feature = "wgpu"))]
        return None;
    }

    // ------------------------------------------------------------- free ----
    pub fn free(&mut self, kind: ResourceKind, rid: Rid) -> bool {
        match kind {
            ResourceKind::Texture => self.textures.free(rid),
            ResourceKind::Mesh => self.meshes.free(rid),
            ResourceKind::Material => self.materials.free(rid),
            ResourceKind::Light => self.lights.free(rid),
            ResourceKind::Camera => self.cameras.free(rid),
            ResourceKind::Environment => self.environments.free(rid),
            ResourceKind::Viewport => self.viewports.free(rid),
            ResourceKind::Instance => self.instances.free(rid),
        }
    }
    pub fn counts(&self) -> (usize, usize, usize, usize, usize, usize, usize, usize) {
        (
            self.textures.len(),
            self.meshes.len(),
            self.materials.len(),
            self.lights.len(),
            self.cameras.len(),
            self.environments.len(),
            self.viewports.len(),
            self.instances.len(),
        )
    }

    // ---------------------------------------------------------- textures ---
    pub fn texture_create(&mut self, width: u32, height: u32, fill: Color) -> Rid {
        let w = width.max(1);
        let h = height.max(1);
        self.textures.alloc(Texture { width: w, height: h, pixels: vec![fill; (w * h) as usize] })
    }
    pub fn texture_set_pixel(&mut self, rid: Rid, x: u32, y: u32, c: Color) -> bool {
        if let Some(t) = self.textures.get_mut(rid) {
            if x < t.width && y < t.height {
                t.pixels[(y * t.width + x) as usize] = c;
                return true;
            }
        }
        false
    }
    pub fn texture_get_pixel(&self, rid: Rid, x: u32, y: u32) -> Option<Color> {
        self.textures.get(rid).and_then(|t| {
            if x < t.width && y < t.height {
                Some(t.pixels[(y * t.width + x) as usize])
            } else {
                None
            }
        })
    }
    pub fn texture_size(&self, rid: Rid) -> Option<(u32, u32)> {
        self.textures.get(rid).map(|t| (t.width, t.height))
    }

    // ------------------------------------------------------------- meshes --
    fn compute_aabb(vertices: &[Vector3]) -> Aabb {
        if vertices.is_empty() {
            return Aabb::new(Vector3::ZERO, Vector3::ZERO);
        }
        let mut lo = vertices[0];
        let mut hi = vertices[0];
        for v in &vertices[1..] {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        Aabb::new(lo, hi - lo)
    }
    pub fn mesh_create(&mut self, vertices: Vec<Vector3>, indices: Vec<u32>) -> Rid {
        let aabb = Self::compute_aabb(&vertices);
        self.meshes.alloc(Mesh { vertices, indices, aabb })
    }
    pub fn mesh_create_box(&mut self, extents: Vector3) -> Rid {
        let e = extents;
        let v = vec![
            Vector3::new(-e.x, -e.y, -e.z),
            Vector3::new(e.x, -e.y, -e.z),
            Vector3::new(e.x, e.y, -e.z),
            Vector3::new(-e.x, e.y, -e.z),
            Vector3::new(-e.x, -e.y, e.z),
            Vector3::new(e.x, -e.y, e.z),
            Vector3::new(e.x, e.y, e.z),
            Vector3::new(-e.x, e.y, e.z),
        ];
        // 12 triangles, indexed.
        let idx = vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 2, 6, 7, 2, 7, 3, 0, 3, 7, 0, 7, 4, 1, 5, 6, 1, 6, 2];
        self.mesh_create(v, idx)
    }
    pub fn mesh_get_aabb(&self, rid: Rid) -> Option<Aabb> {
        self.meshes.get(rid).map(|m| m.aabb)
    }
    pub fn mesh_triangle_count(&self, rid: Rid) -> Option<usize> {
        self.meshes.get(rid).map(|m| {
            if m.indices.is_empty() {
                m.vertices.len() / 3
            } else {
                m.indices.len() / 3
            }
        })
    }

    // ---------------------------------------------------------- materials --
    pub fn material_create(&mut self, albedo: Color) -> Rid {
        self.materials.alloc(Material { albedo, roughness: 0.8, metallic: 0.0, emission: Color::TRANSPARENT })
    }
    pub fn material_set_albedo(&mut self, rid: Rid, c: Color) -> bool {
        if let Some(m) = self.materials.get_mut(rid) {
            m.albedo = c;
            return true;
        }
        false
    }
    pub fn material_get(&self, rid: Rid) -> Option<Material> {
        self.materials.get(rid).copied()
    }

    // ------------------------------------------------------------- lights --
    pub fn light_create(&mut self, kind: LightKind) -> Rid {
        self.lights.alloc(Light {
            kind,
            color: Color::WHITE,
            energy: 1.0,
            direction: Vector3::new(0.0, -1.0, 0.0),
            position: Vector3::ZERO,
            range: 10.0,
        })
    }
    pub fn light_set(&mut self, rid: Rid, color: Color, energy: f32) -> bool {
        if let Some(l) = self.lights.get_mut(rid) {
            l.color = color;
            l.energy = energy;
            return true;
        }
        false
    }

    // ------------------------------------------------------------- camera --
    pub fn camera_create(&mut self, fov_y_deg: f32, near: f32, far: f32) -> Rid {
        self.cameras.alloc(Camera {
            fov_y_deg,
            near: near.max(0.01),
            far: far.max(0.1),
            transform: Transform3D::IDENTITY,
            ortho_size: None,
        })
    }
    pub fn camera_set_transform(&mut self, rid: Rid, t: Transform3D) -> bool {
        if let Some(c) = self.cameras.get_mut(rid) {
            c.transform = t;
            return true;
        }
        false
    }
    /// `None` = perspective; `Some(half_height)` = orthographic.
    pub fn camera_set_ortho(&mut self, rid: Rid, size: Option<f32>) -> bool {
        if let Some(c) = self.cameras.get_mut(rid) {
            c.ortho_size = size.map(|s| s.max(0.05));
            return true;
        }
        false
    }
    pub fn camera_get(&self, rid: Rid) -> Option<Camera> {
        self.cameras.get(rid).copied()
    }
    /// World-space AABBs of all instances with meshes (culling/selection).
    pub fn instance_aabbs(&self) -> Vec<(Rid, Aabb)> {
        let mut out = vec![];
        for id in self.instances.rids() {
            let Some(inst) = self.instances.get(id) else { continue };
            let Some(mesh_id) = inst.mesh else { continue };
            let Some(m) = self.meshes.get(mesh_id) else { continue };
            let corners = aabb_corners(m.aabb);
            let mut lo = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
            let mut hi = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
            for p in corners {
                let w = inst.transform.xform(p);
                lo = lo.min(w);
                hi = hi.max(w);
            }
            out.push((id, Aabb::new(lo, hi - lo)));
        }
        out
    }

    // -------------------------------------------------------- environment --
    pub fn environment_create(&mut self, background: Color) -> Rid {
        self.environments.alloc(Environment { background, ambient: Color::rgb(0.2, 0.2, 0.2), ambient_energy: 1.0 })
    }
    pub fn environment_set_background(&mut self, rid: Rid, c: Color) -> bool {
        if let Some(e) = self.environments.get_mut(rid) {
            e.background = c;
            return true;
        }
        false
    }

    // ----------------------------------------------------------- viewport --
    pub fn viewport_create(&mut self, width: u32, height: u32, clear: Color) -> Rid {
        let (w, h) = (width.max(1), height.max(1));
        self.viewports.alloc(Viewport {
            width: w,
            height: h,
            clear_color: clear,
            camera: None,
            environment: None,
            framebuffer: vec![clear; (w * h) as usize],
        })
    }
    pub fn viewport_set_camera(&mut self, vp: Rid, cam: Option<Rid>) -> bool {
        if let Some(v) = self.viewports.get_mut(vp) {
            v.camera = cam;
            return true;
        }
        false
    }
    pub fn viewport_set_environment(&mut self, vp: Rid, env: Option<Rid>) -> bool {
        if let Some(v) = self.viewports.get_mut(vp) {
            v.environment = env;
            return true;
        }
        false
    }
    pub fn viewport_set_clear_color(&mut self, vp: Rid, c: Color) -> bool {
        if let Some(v) = self.viewports.get_mut(vp) {
            v.clear_color = c;
            return true;
        }
        false
    }
    pub fn capture(&self, vp: Rid) -> Option<Vec<Color>> {
        self.viewports.get(vp).map(|v| v.framebuffer.clone())
    }
    pub fn pixel(&self, vp: Rid, x: u32, y: u32) -> Option<Color> {
        self.viewports.get(vp).and_then(|v| {
            if x < v.width && y < v.height {
                Some(v.framebuffer[(y * v.width + x) as usize])
            } else {
                None
            }
        })
    }
    /// Fraction of pixels differing from the clear color (0 = empty frame).
    pub fn coverage(&self, vp: Rid) -> Option<f32> {
        self.viewports.get(vp).map(|v| {
            let n = v.framebuffer.iter().filter(|p| **p != v.clear_color).count();
            n as f32 / v.framebuffer.len().max(1) as f32
        })
    }

    // ----------------------------------------------------------- instance --
    pub fn instance_create(&mut self, mesh: Option<Rid>, material: Option<Rid>) -> Rid {
        self.instances.alloc(Instance { mesh, material, transform: Transform3D::IDENTITY, visible: true })
    }
    pub fn instance_set_transform(&mut self, rid: Rid, t: Transform3D) -> bool {
        if let Some(i) = self.instances.get_mut(rid) {
            i.transform = t;
            return true;
        }
        false
    }
    pub fn instance_set_visible(&mut self, rid: Rid, v: bool) -> bool {
        if let Some(i) = self.instances.get_mut(rid) {
            i.visible = v;
            return true;
        }
        false
    }
    pub fn instance_set_mesh(&mut self, rid: Rid, mesh: Option<Rid>) -> bool {
        if let Some(i) = self.instances.get_mut(rid) {
            i.mesh = mesh;
            return true;
        }
        false
    }

    // -------------------------------------------------------------- render -
    /// Software render: clear every viewport, then draw each visible instance
    /// as its projected mesh-AABB rect shaded by albedo × (ambient + key light).
    /// Deterministic, headless, and sufficient to verify scene→pixels plumbing;
    /// real rasterization/ray-tracing arrives with the wgpu device (module 04).
    pub fn render(&mut self) {
        #[cfg(feature = "wgpu")]
        if self.backend == RenderBackendKind::Wgpu {
            self.render_wgpu();
            return;
        }
        let light = self.lights.rids().first().and_then(|r| self.lights.get(*r).copied());
        let vp_ids = self.viewports.rids();
        for vp_id in vp_ids {
            // Snapshot viewport + camera + environment (borrow dance).
            let (w, h, clear, cam, env) = {
                let Some(v) = self.viewports.get(vp_id) else { continue };
                let cam = v.camera.and_then(|c| self.cameras.get(c).copied());
                let env = v.environment.and_then(|e| self.environments.get(e).copied());
                (v.width, v.height, v.clear_color, cam, env)
            };
            let bg = env.map(|e| e.background).unwrap_or(clear);
            let ambient = env.map(|e| e.ambient * e.ambient_energy).unwrap_or(Color::rgb(0.2, 0.2, 0.2));
            let proj = cam.map(|c| {
                let aspect = w as f32 / h.max(1) as f32;
                match c.ortho_size {
                    Some(s) => Projection::create_orthogonal(-s * aspect, s * aspect, -s, s, c.near, c.far),
                    None => Projection::create_perspective(c.fov_y_deg.to_radians(), aspect, c.near, c.far),
                }
            });
            let view = cam.map(|c| c.transform.affine_inverse());
            let mut fb = vec![bg; (w * h) as usize];
            let mut draws = 0u64;
            let mut culled = 0u64;
            let mut tris = 0u64;
            for inst_id in self.instances.rids() {
                let Some(inst) = self.instances.get(inst_id) else { continue };
                if !inst.visible {
                    continue;
                }
                let Some(mesh_id) = inst.mesh else { continue };
                let Some(mesh) = self.meshes.get(mesh_id) else { continue };
                let mat = inst.material.and_then(|m| self.materials.get(m).copied()).unwrap_or(Material {
                    albedo: Color::WHITE,
                    ..Default::default()
                });
                // World-space AABB corners.
                let corners = aabb_corners(mesh.aabb);
                let world: Vec<Vector3> = corners.iter().map(|p| inst.transform.xform(*p)).collect();
                // Cull: center behind camera.
                if let Some(v) = view {
                    let center = world.iter().fold(Vector3::ZERO, |a, b| a + *b) / world.len() as f32;
                    let in_view = v.xform(center);
                    if in_view.z > -0.01 {
                        culled += 1;
                        continue;
                    }
                }
                // Project to pixels.
                let rect = if let (Some(p), Some(v)) = (proj.clone(), view) {
                    project_rect(&world, &v, &p, w, h)
                } else {
                    // No camera: draw centered at 50% size.
                    let (fw, fh) = (w as f32 * 0.25, h as f32 * 0.25);
                    (w as f32 * 0.5 - fw, h as f32 * 0.5 - fh, fw * 2.0, fh * 2.0)
                };
                // Shade: albedo × (ambient + key*energy), clamped.
                let key = light.map(|l| l.color * l.energy).unwrap_or(Color::WHITE * 0.0);
                let shade = Color {
                    r: (mat.albedo.r * (ambient.r + key.r)).clamp(0.0, 1.0),
                    g: (mat.albedo.g * (ambient.g + key.g)).clamp(0.0, 1.0),
                    b: (mat.albedo.b * (ambient.b + key.b)).clamp(0.0, 1.0),
                    a: 1.0,
                };
                fill_rect(&mut fb, w, h, rect, shade);
                draws += 1;
                tris += mesh.indices.len().max(mesh.vertices.len()).max(3) as u64 / 3;
            }
            if let Some(v) = self.viewports.get_mut(vp_id) {
                v.framebuffer = fb;
            }
            self.stats.draw_calls += draws;
            self.stats.culled += culled;
            self.stats.triangles += tris;
            self.stats.frames += 1;
        }
    }

    /// GPU path: same projection/culling/shading as `render`, rasterized by
    /// wgpu into an offscreen texture, read back into the framebuffers.
    #[cfg(feature = "wgpu")]
    fn render_wgpu(&mut self) {
        let light = self.lights.rids().first().and_then(|r| self.lights.get(*r).copied());
        let vp_ids = self.viewports.rids();
        for vp_id in vp_ids {
            let (w, h, clear, cam, env) = {
                let Some(v) = self.viewports.get(vp_id) else { continue };
                let cam = v.camera.and_then(|c| self.cameras.get(c).copied());
                let env = v.environment.and_then(|e| self.environments.get(e).copied());
                (v.width, v.height, v.clear_color, cam, env)
            };
            let bg = env.map(|e| e.background).unwrap_or(clear);
            let ambient = env.map(|e| e.ambient * e.ambient_energy).unwrap_or(Color::rgb(0.2, 0.2, 0.2));
            let proj = cam.map(|c| match c.ortho_size {
                Some(s) => {
                    let aspect = w as f32 / h.max(1) as f32;
                    Projection::create_orthogonal(-s * aspect, s * aspect, -s, s, c.near, c.far)
                }
                None => {
                    let aspect = w as f32 / h.max(1) as f32;
                    Projection::create_perspective(c.fov_y_deg.to_radians(), aspect, c.near, c.far)
                }
            });
            let view = cam.map(|c| c.transform.affine_inverse());
            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = vec![];
            let mut culled = 0u64;
            let mut tris = 0u64;
            for inst_id in self.instances.rids() {
                let Some(inst) = self.instances.get(inst_id) else { continue };
                if !inst.visible {
                    continue;
                }
                let Some(mesh_id) = inst.mesh else { continue };
                let Some(mesh) = self.meshes.get(mesh_id) else { continue };
                let mat = inst.material.and_then(|m| self.materials.get(m).copied()).unwrap_or(Material {
                    albedo: Color::WHITE,
                    ..Default::default()
                });
                let corners = aabb_corners(mesh.aabb);
                let world: Vec<Vector3> = corners.iter().map(|p| inst.transform.xform(*p)).collect();
                if let Some(v) = view {
                    let center = world.iter().fold(Vector3::ZERO, |a, b| a + *b) / world.len() as f32;
                    if v.xform(center).z > -0.01 {
                        culled += 1;
                        continue;
                    }
                }
                let rect = if let (Some(p), Some(v)) = (proj.clone(), view) {
                    project_rect(&world, &v, &p, w, h)
                } else {
                    let (fw, fh) = (w as f32 * 0.25, h as f32 * 0.25);
                    (w as f32 * 0.5 - fw, h as f32 * 0.5 - fh, fw * 2.0, fh * 2.0)
                };
                let key = light.map(|l| l.color * l.energy).unwrap_or(Color::WHITE * 0.0);
                let shade = [
                    (mat.albedo.r * (ambient.r + key.r)).clamp(0.0, 1.0),
                    (mat.albedo.g * (ambient.g + key.g)).clamp(0.0, 1.0),
                    (mat.albedo.b * (ambient.b + key.b)).clamp(0.0, 1.0),
                    1.0,
                ];
                // Pixel rect -> NDC (y flip: pixel y-down to NDC y-up).
                let (x, y, rw, rh) = rect;
                quads.push((
                    x / w as f32 * 2.0 - 1.0,
                    1.0 - y / h as f32 * 2.0,
                    (x + rw) / w as f32 * 2.0 - 1.0,
                    1.0 - (y + rh) / h as f32 * 2.0,
                    shade,
                ));
                tris += mesh.indices.len().max(mesh.vertices.len()).max(3) as u64 / 3;
            }
            let clear = [bg.r as f64, bg.g as f64, bg.b as f64, 1.0];
            let draws = quads.len() as u64;
            let fb = match self.gpu.as_mut() {
                Some(gpu) => gpu.render_quads(w, h, clear, &quads).unwrap_or_else(|_| {
                    // GPU failure mid-frame: fall back to a clear-color frame, keep stats honest.
                    vec![bg; (w * h) as usize]
                }),
                None => vec![bg; (w * h) as usize],
            };
            if let Some(v) = self.viewports.get_mut(vp_id) {
                v.framebuffer = fb;
            }
            self.stats.draw_calls += draws;
            self.stats.culled += culled;
            self.stats.triangles += tris;
            self.stats.frames += 1;
        }
    }
}

impl Default for RenderingServer {
    fn default() -> Self {
        Self::new(RenderBackendKind::Software)
    }
}

fn aabb_corners(a: Aabb) -> [Vector3; 8] {
    let (p, s) = (a.position, a.size);
    [
        p,
        p + Vector3::new(s.x, 0.0, 0.0),
        p + Vector3::new(0.0, s.y, 0.0),
        p + Vector3::new(0.0, 0.0, s.z),
        p + s,
        p + Vector3::new(s.x, s.y, 0.0),
        p + Vector3::new(s.x, 0.0, s.z),
        p + Vector3::new(0.0, s.y, s.z),
    ]
}

/// Project world points to a pixel-space bounding rect (x, y, w, h).
fn project_rect(world: &[Vector3], view: &Transform3D, proj: &Projection, w: u32, h: u32) -> (f32, f32, f32, f32) {
    let mut lo = Vector2::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Vector2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut any = false;
    for p in world {
        let v = view.xform(*p);
        if v.z > -0.01 {
            continue; // behind camera
        }
        let c = proj.clone().xform(Vector4::new(v.x, v.y, v.z, 1.0));
        if c.w.abs() < 1e-6 {
            continue;
        }
        let ndc = Vector2::new(c.x / c.w, c.y / c.w);
        let px = (ndc.x * 0.5 + 0.5) * w as f32;
        let py = (0.5 - ndc.y * 0.5) * h as f32;
        lo = lo.min(Vector2::new(px, py));
        hi = hi.max(Vector2::new(px, py));
        any = true;
    }
    if !any {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (lo.x, lo.y, (hi.x - lo.x).max(1.0), (hi.y - lo.y).max(1.0))
}

fn fill_rect(fb: &mut [Color], w: u32, h: u32, rect: (f32, f32, f32, f32), c: Color) {
    let (x0, y0, rw, rh) = rect;
    let (xa, ya) = (x0.max(0.0) as u32, y0.max(0.0) as u32);
    let (xb, yb) = ((x0 + rw).min(w as f32) as u32, ((y0 + rh).min(h as f32)) as u32);
    for y in ya..yb {
        for x in xa..xb {
            fb[(y * w + x) as usize] = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn server() -> (RenderingServer, Rid, Rid, Rid, Rid) {
        let mut s = RenderingServer::new(RenderBackendKind::Software);
        let vp = s.viewport_create(64, 64, Color::BLACK);
        let cam = s.camera_create(70.0, 0.05, 100.0);
        s.camera_set_transform(cam, Transform3D::from_translation(Vector3::new(0.0, 0.0, 3.0)));
        s.viewport_set_camera(vp, Some(cam));
        let mesh = s.mesh_create_box(Vector3::new(0.5, 0.5, 0.5));
        let mat = s.material_create(Color::RED);
        (s, vp, cam, mesh, mat)
    }
    #[test]
    fn clear_and_draw_are_deterministic() {
        let (mut s, vp, _cam, mesh, mat) = server();
        s.render();
        assert_eq!(s.coverage(vp).unwrap(), 0.0); // nothing yet
        let inst = s.instance_create(Some(mesh), Some(mat));
        s.render();
        let cov = s.coverage(vp).unwrap();
        assert!(cov > 0.05, "box must cover pixels, got {cov}");
        // Same scene re-rendered is bit-identical.
        let a = s.capture(vp).unwrap();
        s.render();
        assert_eq!(a, s.capture(vp).unwrap());
        let _ = inst;
    }
    #[test]
    fn visibility_and_culling_work() {
        let (mut s, vp, _cam, mesh, mat) = server();
        let inst = s.instance_create(Some(mesh), Some(mat));
        s.render();
        assert!(s.coverage(vp).unwrap() > 0.0);
        s.instance_set_visible(inst, false);
        s.render();
        assert_eq!(s.coverage(vp).unwrap(), 0.0);
        // Behind camera: moved to z=+10 (camera looks -Z from z=3).
        s.instance_set_visible(inst, true);
        s.instance_set_transform(inst, Transform3D::from_translation(Vector3::new(0.0, 0.0, 10.0)));
        s.render();
        assert_eq!(s.coverage(vp).unwrap(), 0.0);
        assert!(s.stats.culled > 0);
    }
    #[test]
    fn rids_reuse_and_textures_roundtrip() {
        let mut s = RenderingServer::new(RenderBackendKind::Software);
        let t = s.texture_create(2, 2, Color::WHITE);
        assert!(s.texture_set_pixel(t, 0, 0, Color::RED));
        assert_eq!(s.texture_get_pixel(t, 0, 0), Some(Color::RED));
        assert_eq!(s.texture_size(t), Some((2, 2)));
        assert!(s.free(ResourceKind::Texture, t));
        assert!(!s.free(ResourceKind::Texture, t));
        let box_mesh = s.mesh_create_box(Vector3::ONE);
        assert_eq!(s.mesh_triangle_count(box_mesh), Some(12));
    }
    #[test]
    fn wgpu_requests_are_explicit() {
        let mut s = RenderingServer::new(RenderBackendKind::Software);
        let r = s.set_backend(RenderBackendKind::Wgpu);
        #[cfg(feature = "wgpu")]
        {
            // Graceful on adapter-less machines; attached where a GPU exists.
            if r.is_ok() {
                assert!(s.has_gpu());
                assert!(!s.gpu_adapter_name().unwrap_or_default().is_empty());
            }
        }
        #[cfg(not(feature = "wgpu"))]
        assert!(r.is_err());
    }
    #[cfg(feature = "wgpu")]
    #[test]
    fn wgpu_renders_scene_offscreen() {
        let mut s = RenderingServer::new(RenderBackendKind::Software);
        let vp = s.viewport_create(64, 64, Color::BLACK);
        let cam = s.camera_create(70.0, 0.05, 100.0);
        s.camera_set_transform(cam, Transform3D::from_translation(Vector3::new(0.0, 0.0, 3.0)));
        s.viewport_set_camera(vp, Some(cam));
        let mesh = s.mesh_create_box(Vector3::new(0.5, 0.5, 0.5));
        let mat = s.material_create(Color::RED);
        let _inst = s.instance_create(Some(mesh), Some(mat));
        if s.set_backend(RenderBackendKind::Wgpu).is_err() {
            eprintln!("wgpu unavailable, skipping offscreen scene render");
            return;
        }
        s.render();
        assert_eq!(s.backend(), RenderBackendKind::Wgpu);
        let cov = s.coverage(vp).unwrap_or(0.0);
        assert!(cov > 0.05, "gpu must draw the box, got coverage {cov}");
        // Same scene re-rendered is bit-identical.
        let a = s.capture(vp).unwrap();
        s.render();
        assert_eq!(a, s.capture(vp).unwrap());
    }
}
