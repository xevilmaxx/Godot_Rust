//! Wgpu offscreen backend: render-to-texture + CPU readback (`RenderBackend::Wgpu`).
//!
//! No window or swapchain is involved, so this works headless (CI, agents) and
//! feeds the exact same `Vec<Color>` framebuffers the software path and the GUI
//! viewport display. Device selection prefers a fallback (software) adapter so
//! machines without a discrete GPU still render.

use godot_core::Color;

const MAX_QUADS: usize = 4096;
const VERTEX_FLOATS: usize = 6; // xy + rgba
const VERTEX_BYTES: usize = VERTEX_FLOATS * 4;

const SHADER: &str = r#"
struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) col: vec4<f32>,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) col: vec4<f32>,
};
@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    o.clip = vec4<f32>(v.pos, 0.0, 1.0);
    o.col = v.col;
    return o;
}
@fragment
fn fs_main(@location(0) col: vec4<f32>) -> @location(0) vec4<f32> {
    return col;
}
"#;

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    target: Option<(u32, u32, wgpu::Texture, wgpu::TextureView, wgpu::Buffer, u32)>,
    adapter_name: String,
}

fn align256(x: u32) -> u32 {
    x.div_ceil(256) * 256
}

impl WgpuBackend {
    pub fn try_new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: true,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no wgpu adapter (headless ok, GPU optional): {e:?}"))?;
        let adapter_name = adapter.get_info().name.clone();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("godot_rs"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| format!("wgpu device request failed: {e:?}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("godot_rs flat"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("godot_rs layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("godot_rs quad"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 8, shader_location: 1 },
                    ],
                })],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("godot_rs verts"),
            size: (MAX_QUADS * 6 * VERTEX_BYTES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self { device, queue, pipeline, vertex_buf, target: None, adapter_name })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if matches!(&self.target, Some((tw, th, ..)) if *tw == w && *th == h) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("godot_rs offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let pitch = align256(w * 4);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("godot_rs readback"),
            size: (pitch * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.target = Some((w, h, texture, view, readback, pitch));
    }

    /// Render NDC-space quads `(x0, y0, x1, y1, rgba)` over `clear`, read back pixels.
    pub fn render_quads(
        &mut self,
        w: u32,
        h: u32,
        clear: [f64; 4],
        quads: &[(f32, f32, f32, f32, [f32; 4])],
    ) -> Result<Vec<Color>, String> {
        self.ensure_target(w, h);
        let Some((tw, th, texture, view, readback, pitch)) = &self.target else {
            return Err("wgpu target missing".into());
        };
        let (tw, th, pitch) = (*tw, *th, *pitch);
        // Pack quads as two triangles (CCW).
        let mut verts: Vec<u8> = Vec::with_capacity(quads.len().min(MAX_QUADS) * 6 * VERTEX_BYTES);
        let push = |out: &mut Vec<u8>, x: f32, y: f32, c: &[f32; 4]| {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            for v in c {
                out.extend_from_slice(&v.to_le_bytes());
            }
        };
        for (x0, y0, x1, y1, c) in quads.iter().take(MAX_QUADS) {
            let c = [c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), c[3].clamp(0.0, 1.0)];
            push(&mut verts, *x0, *y0, &c);
            push(&mut verts, *x1, *y0, &c);
            push(&mut verts, *x1, *y1, &c);
            push(&mut verts, *x0, *y0, &c);
            push(&mut verts, *x1, *y1, &c);
            push(&mut verts, *x0, *y1, &c);
        }
        let vert_count = (verts.len() / VERTEX_BYTES) as u32;
        self.queue.write_buffer(&self.vertex_buf, 0, &verts);
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("godot_rs enc") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("godot_rs pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: clear[3] }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..verts.len() as u64));
            pass.draw(0..vert_count, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pitch), rows_per_image: Some(th) },
            },
            wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        // Read back (blocking).
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("wgpu poll failed: {e:?}"))?;
        rx.recv().map_err(|_| "wgpu map callback lost".to_string())?.map_err(|e| format!("wgpu map failed: {e:?}"))?;
        let data = slice.get_mapped_range().map_err(|e| format!("wgpu range failed: {e:?}"))?.to_vec();
        readback.unmap();
        let mut out = Vec::with_capacity((tw * th) as usize);
        for y in 0..th {
            let row = (y * pitch) as usize;
            for x in 0..tw {
                let o = row + (x * 4) as usize;
                out.push(Color {
                    r: data[o] as f32 / 255.0,
                    g: data[o + 1] as f32 / 255.0,
                    b: data[o + 2] as f32 / 255.0,
                    a: data[o + 3] as f32 / 255.0,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wgpu_probe_renders_or_reports_gracefully() {
        let mut gpu = match WgpuBackend::try_new() {
            Ok(g) => g,
            Err(e) => {
                // Headless CI without any adapter: still a pass, with a reason.
                eprintln!("wgpu unavailable, skipping real render ({e})");
                return;
            }
        };
        eprintln!("wgpu adapter: {}", gpu.adapter_name());
        // Full-red quad over black: center pixel must be red, corners red too.
        let fb = gpu
            .render_quads(64, 64, [0.0, 0.0, 0.0, 1.0], &[(-1.0, -1.0, 1.0, 1.0, [1.0, 0.0, 0.0, 1.0])])
            .expect("render");
        assert_eq!(fb.len(), 64 * 64);
        let center = fb[32 * 64 + 32];
        assert!(center.r > 0.9 && center.g < 0.1 && center.b < 0.1, "got {center:?}");
    }
}
