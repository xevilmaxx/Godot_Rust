//! MCP server (JSON-RPC 2.0 over stdio, like SolidCraft `solidcraft-cli mcp`).
//! Tools: scene (list_commands/execute/inspect/set_parameter/export) +
//! servers (render/physics_step/audio_sine/wav) — all against live state.

use godot_core::Variant;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub struct McpServer {
    pub tools: Vec<ToolDef>,
    tree: std::sync::Mutex<godot_scene::SceneTree>,
    rendering: std::sync::Mutex<godot_servers::RenderingServer>,
    physics3d: std::sync::Mutex<godot_servers::PhysicsServer3D>,
    physics2d: std::sync::Mutex<godot_servers::PhysicsServer2D>,
    audio: std::sync::Mutex<godot_servers::AudioServer>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            tools: vec![
                ToolDef {
                    name: "list_commands".into(),
                    description: "List engine commands and their arguments.".into(),
                    input_schema: serde_json::json!({"type":"object","properties":{}}),
                },
                ToolDef {
                    name: "execute".into(),
                    description: "Run an engine command (add_node, set_prop, …).".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"command":{"type":"string"},"args":{"type":"object"}},
                        "required":["command"]
                    }),
                },
                ToolDef {
                    name: "inspect".into(),
                    description: "Return ClassDB JSON + scene summary.".into(),
                    input_schema: serde_json::json!({"type":"object","properties":{}}),
                },
                ToolDef {
                    name: "set_parameter".into(),
                    description: "Set a node property by id.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"node":{"type":"integer"},"property":{"type":"string"},"value":{}},
                        "required":["node","property","value"]
                    }),
                },
                ToolDef {
                    name: "export".into(),
                    description: "Export scene JSON or mixed WAV audio.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"format":{"type":"string","enum":["json","wav"]}}
                    }),
                },
                ToolDef {
                    name: "render".into(),
                    description: "Render all viewports (software) and return draw stats.".into(),
                    input_schema: serde_json::json!({"type":"object","properties":{}}),
                },
                ToolDef {
                    name: "render_demo".into(),
                    description: "Build a demo viewport+box, render it, return pixel coverage.".into(),
                    input_schema: serde_json::json!({"type":"object","properties":{}}),
                },
                ToolDef {
                    name: "physics3d_step".into(),
                    description: "Create a falling-ball space if needed, step it, return height+contacts.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"steps":{"type":"integer"},"dt":{"type":"number"}}
                    }),
                },
                ToolDef {
                    name: "physics2d_step".into(),
                    description: "Create a falling-circle space if needed, step it, return height+contacts.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"steps":{"type":"integer"},"dt":{"type":"number"}}
                    }),
                },
                ToolDef {
                    name: "audio_sine".into(),
                    description: "Mix a sine tone and return sample count, zero crossings, WAV bytes.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"freq":{"type":"number"},"seconds":{"type":"number"}}
                    }),
                },
                ToolDef {
                    name: "iris_doc".into(),
                    description: "Parse Iris source and return its machine-readable schema (props/fns/grammar).".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"source":{"type":"string"}},
                        "required":["source"]
                    }),
                },
                ToolDef {
                    name: "nav_path".into(),
                    description: "2D grid A* demo: wall with gap, return waypoint count and length.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"width":{"type":"integer"},"height":{"type":"integer"}}
                    }),
                },
                ToolDef {
                    name: "render_gpu".into(),
                    description: "Render the demo scene with the wgpu backend; needs godot_servers/wgpu.".into(),
                    input_schema: serde_json::json!({"type":"object","properties":{}}),
                },
                ToolDef {
                    name: "nav_bake".into(),
                    description: "Bake obstacle rects into a 2D nav grid and route around them.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"rects":{"type":"array","items":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4}}}
                    }),
                },
                ToolDef {
                    name: "import_gltf".into(),
                    description: "Import a .gltf/.glb file; return meshes/nodes/materials.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"path":{"type":"string"}},
                        "required":["path"]
                    }),
                },
                ToolDef {
                    name: "gd_transpile".into(),
                    description: "Transpile GDScript source to Iris; return schema + emitted source.".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"source":{"type":"string"}},
                        "required":["source"]
                    }),
                },
            ],
            tree: std::sync::Mutex::new(godot_scene::SceneTree::new()),
            rendering: std::sync::Mutex::new(godot_servers::RenderingServer::default()),
            physics3d: std::sync::Mutex::new(godot_servers::PhysicsServer3D::new()),
            physics2d: std::sync::Mutex::new(godot_servers::PhysicsServer2D::new()),
            audio: std::sync::Mutex::new(godot_servers::AudioServer::new()),
        }
    }

    /// Live scene behind the server (agents mutate the same tree the editor shows).
    pub fn tree(&self) -> std::sync::MutexGuard<'_, godot_scene::SceneTree> {
        self.tree.lock().expect("mcp tree poisoned")
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.clone()).collect()
    }

    /// Dispatch one JSON-RPC call against the live `SceneTree`; returns `result`.
    pub fn dispatch(&self, method: &str, params: &Value) -> Value {
        match method {
            "list_commands" | "tools/list" => {
                serde_json::to_value(&self.tools).unwrap_or(Value::Null)
            }
            "inspect" => self.tree().inspect_summary(),
            "execute" => {
                let cmd = params.get("command").and_then(|c| c.as_str()).unwrap_or("");
                let args = params.get("args").cloned().unwrap_or_else(|| params.clone());
                // Accept both `{command, args}` and `{command, ...direct args}`.
                let merged = if args.get("command").is_some() { args } else { params.clone() };
                match self.tree().execute(cmd, &merged) {
                    Ok(v) => {
                        let mut o = serde_json::json!({"ok": true, "command": cmd});
                        o["result"] = v;
                        o
                    }
                    Err(e) => serde_json::json!({"error": e.to_string()}),
                }
            }
            "set_parameter" => {
                let node = params.get("node").and_then(|v| v.as_u64()).unwrap_or(0);
                let prop = params.get("property").and_then(|v| v.as_str()).unwrap_or("");
                let value = params.get("value").cloned().unwrap_or(Value::Null);
                match self
                    .tree()
                    .execute("set_parameter", &serde_json::json!({"node": node, "property": prop, "value": value}))
                {
                    Ok(_) => serde_json::json!({"ok": true, "node": node, "property": prop}),
                    Err(e) => serde_json::json!({"error": e.to_string()}),
                }
            }
            "export" => {
                let fmt = params.get("format").and_then(|v| v.as_str()).unwrap_or("json");
                match fmt {
                    "json" => serde_json::json!({"ok": true, "scene": self.tree().to_json()}),
                    "wav" => {
                        let freq = params.get("freq").and_then(|v| v.as_f64()).unwrap_or(440.0);
                        let seconds = params.get("seconds").and_then(|v| v.as_f64()).unwrap_or(0.5);
                        let mut audio = self.audio.lock().expect("audio poisoned");
                        let p = audio.player_create(
                            godot_servers::audio::Stream::Sine { freq: freq as f32 },
                            0,
                        );
                        audio.play(p, 0.0);
                        let pcm = audio.render_pcm(seconds);
                        let wav = godot_servers::AudioServer::encode_wav(&pcm);
                        serde_json::json!({"ok": true, "samples": pcm.len(), "wav_bytes": wav.len()})
                    }
                    other => serde_json::json!({"error": format!("unsupported export format {other}")}),
                }
            }
            "render" => {
                let mut r = self.rendering.lock().expect("rendering poisoned");
                r.render();
                serde_json::json!({
                    "ok": true,
                    "draw_calls": r.stats.draw_calls,
                    "culled": r.stats.culled,
                    "triangles": r.stats.triangles,
                    "frames": r.stats.frames,
                })
            }
            "render_demo" => {
                use godot_core::{Color, Transform3D, Vector3};
                let mut r = self.rendering.lock().expect("rendering poisoned");
                let vp = r.viewport_create(64, 64, Color::BLACK);
                let cam = r.camera_create(70.0, 0.05, 100.0);
                r.camera_set_transform(cam, Transform3D::from_translation(Vector3::new(0.0, 0.0, 3.0)));
                r.viewport_set_camera(vp, Some(cam));
                let mesh = r.mesh_create_box(Vector3::new(0.5, 0.5, 0.5));
                let mat = r.material_create(Color::RED);
                let _inst = r.instance_create(Some(mesh), Some(mat));
                r.render();
                serde_json::json!({"ok": true, "viewport": vp.0, "coverage": r.coverage(vp)})
            }
            "physics3d_step" => {
                use godot_core::Vector3;
                let steps = params.get("steps").and_then(|v| v.as_u64()).unwrap_or(120) as usize;
                let dt = params.get("dt").and_then(|v| v.as_f64()).unwrap_or(1.0 / 60.0);
                let mut p = self.physics3d.lock().expect("physics poisoned");
                let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
                let ball = p.body_create(space, godot_servers::physics3d::BodyMode3D::Rigid, 1.0).unwrap();
                p.body_add_shape(space, ball, godot_servers::physics3d::Shape3D::Sphere { radius: 0.5 });
                p.body_set_position(space, ball, Vector3::new(0.0, 5.0, 0.0));
                for _ in 0..steps.min(3600) {
                    p.space_step(space, dt);
                }
                let y = p.body_get_position(space, ball).map(|v| v.y).unwrap_or(f32::NAN);
                serde_json::json!({"ok": true, "y": y, "contacts": p.contact_count(space)})
            }
            "physics2d_step" => {
                use godot_core::Vector2;
                let steps = params.get("steps").and_then(|v| v.as_u64()).unwrap_or(120) as usize;
                let dt = params.get("dt").and_then(|v| v.as_f64()).unwrap_or(1.0 / 60.0);
                let mut p = self.physics2d.lock().expect("physics2d poisoned");
                let space = p.space_create(Vector2::new(0.0, 980.0));
                let ball = p.body_create(space, godot_servers::physics2d::BodyMode2D::Rigid, 1.0).unwrap();
                p.body_add_shape(space, ball, godot_servers::physics2d::Shape2D::Circle { radius: 10.0 });
                p.body_set_position(space, ball, Vector2::new(0.0, -200.0));
                for _ in 0..steps.min(3600) {
                    p.space_step(space, dt);
                }
                let y = p.body_get_position(space, ball).map(|v| v.y).unwrap_or(f32::NAN);
                serde_json::json!({"ok": true, "y": y, "contacts": p.contact_count(space)})
            }
            "iris_doc" => {
                let src = params.get("source").and_then(|v| v.as_str()).unwrap_or("");
                match godot_script_iris::parse(src) {
                    Ok(f) => {
                        let diags = godot_script_iris::check(&f);
                        let doc = godot_script_iris::doc_json(&f);
                        serde_json::json!({"ok": true, "doc": doc, "diagnostics": diags})
                    }
                    Err(e) => serde_json::json!({"error": e.to_string()}),
                }
            }
            "nav_path" => {
                use godot_core::Vector2;
                let w = params.get("width").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
                let h = params.get("height").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
                let mut nav = godot_nav::NavigationServer2D::new();
                let map = nav.map_create(w.max(3), h.max(3), 1.0, true);
                let mid = w.max(3) / 2;
                for y in 0..h.max(3).saturating_sub(1) {
                    nav.map_set_blocked(map, mid, y, true);
                }
                let path = nav.map_get_path(map, Vector2::new(0.5, 4.5), Vector2::new(w as f32 - 0.5, 4.5));
                let len: f32 = path.windows(2).map(|s| (s[1] - s[0]).length()).sum();
                serde_json::json!({"ok": true, "waypoints": path.len(), "length": len})
            }
            "render_gpu" => {
                use godot_core::{Color, Transform3D, Vector3};
                let mut r = self.rendering.lock().expect("rendering poisoned");
                if let Err(e) = r.set_backend(godot_servers::RenderBackendKind::Wgpu) {
                    return serde_json::json!({"error": e});
                }
                let vp = r.viewport_create(64, 64, Color::BLACK);
                let cam = r.camera_create(70.0, 0.05, 100.0);
                r.camera_set_transform(cam, Transform3D::from_translation(Vector3::new(0.0, 0.0, 3.0)));
                r.viewport_set_camera(vp, Some(cam));
                let mesh = r.mesh_create_box(Vector3::new(0.5, 0.5, 0.5));
                let mat = r.material_create(Color::RED);
                let _inst = r.instance_create(Some(mesh), Some(mat));
                r.render();
                serde_json::json!({
                    "ok": true,
                    "backend": format!("{:?}", r.backend()),
                    "gpu": r.gpu_adapter_name(),
                    "coverage": r.coverage(vp),
                    "draws": r.stats.draw_calls,
                })
            }
            "nav_bake" => {
                use godot_core::Vector2;
                let rects: Vec<(Vector2, Vector2)> = params
                    .get("rects")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| r.as_array())
                            .filter_map(|r| {
                                let n: Vec<f64> = r.iter().filter_map(|x| x.as_f64()).collect();
                                if n.len() == 4 {
                                    Some((Vector2::new(n[0] as f32, n[1] as f32), Vector2::new(n[2] as f32, n[3] as f32)))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_else(|| vec![(Vector2::new(4.5, 4.5), Vector2::new(1.0, 4.5))]);
                let mut nav = godot_nav::NavigationServer2D::new();
                let map = nav.map_create(10, 10, 1.0, true);
                let blocked = nav.bake_rects(map, &rects);
                let path = nav.map_get_path(map, Vector2::new(0.5, 4.5), Vector2::new(9.5, 4.5));
                serde_json::json!({"ok": true, "blocked": blocked, "waypoints": path.len()})
            }
            "import_gltf" => {
                let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
                match godot_io::GltfDocument::from_file(path) {
                    Ok(doc) => serde_json::json!({
                        "ok": true,
                        "meshes": doc.meshes.len(),
                        "nodes": doc.nodes.len(),
                        "materials": doc.materials.len(),
                        "generator": doc.generator,
                        "vertices": doc.meshes.first().map(|m| m.positions.len()).unwrap_or(0),
                    }),
                    Err(e) => serde_json::json!({"error": e.to_string()}),
                }
            }
            "gd_transpile" => {
                let src = params.get("source").and_then(|v| v.as_str()).unwrap_or("");
                match godot_script_iris::gdtranspile::transpile(src) {
                    Ok(f) => {
                        let doc = godot_script_iris::doc_json(&f);
                        serde_json::json!({
                            "ok": true,
                            "node_class": doc.node_class,
                            "base_class": doc.base_class,
                            "props": doc.props.len(),
                            "fns": doc.fns.len(),
                            "iris": godot_script_iris::gdtranspile::emit_iris(&f),
                        })
                    }
                    Err(e) => serde_json::json!({"error": e.to_string()}),
                }
            }
            "audio_sine" => {
                let freq = params.get("freq").and_then(|v| v.as_f64()).unwrap_or(440.0) as f32;
                let seconds = params.get("seconds").and_then(|v| v.as_f64()).unwrap_or(0.1);
                let mut a = self.audio.lock().expect("audio poisoned");
                let pl = a.player_create(godot_servers::audio::Stream::Sine { freq }, 0);
                a.play(pl, 0.0);
                let pcm = a.render_pcm(seconds);
                let zc = pcm.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
                let wav = godot_servers::AudioServer::encode_wav(&pcm);
                serde_json::json!({"ok": true, "samples": pcm.len(), "zero_crossings": zc, "wav_bytes": wav.len()})
            }
            "shutdown" => serde_json::json!({"ok": true}),
            _ => serde_json::json!({"error": format!("unknown tool {method}")}),
        }
    }

    /// Minimal JSON-RPC 2.0 stdio loop: `{"jsonrpc":"2.0","id":1,"method":"inspect","params":{}}`.
    /// Used by `godot_rs_editor mcp` and any MCP client over stdio.
    pub fn serve_stdio(&self) -> Result<(), McpError> {
        use std::io::{BufRead, Write};
        let stdin = std::io::stdin();
        let mut out = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if line.trim().is_empty() {
                continue;
            }
            let req: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let err = serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}});
                    writeln!(out, "{}", err)?;
                    continue;
                }
            };
            let id = req.get("id").cloned().unwrap_or(Value::Null);
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let params = req.get("params").cloned().unwrap_or(Value::Null);
            if method == "notifications/initialized" || method.starts_with("notifications/") {
                continue;
            }
            if method == "initialize" {
                let res = serde_json::json!({
                    "jsonrpc":"2.0","id":id,
                    "result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"godot_rs","version":"0.1.0"}}
                });
                writeln!(out, "{}", res)?;
                continue;
            }
            let result = self.dispatch(method, &params);
            let is_err = result.get("error").is_some();
            let res = if is_err {
                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":result}})
            } else {
                serde_json::json!({"jsonrpc":"2.0","id":id,"result":result})
            };
            writeln!(out, "{}", res)?;
            out.flush().ok();
        }
        Ok(())
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct McpError(pub String);

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mcp: {}", self.0)
    }
}

impl From<std::io::Error> for McpError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<serde_json::Error> for McpError {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_tools_answer() {
        let server = McpServer::new();
        // nav_bake with an explicit wall.
        let baked = server.dispatch("nav_bake", &serde_json::json!({"rects": [[4.5, 4.5, 1.0, 4.5]]}));
        assert_eq!(baked["ok"], true);
        assert!(baked["blocked"].as_u64().unwrap_or(0) > 0);
        assert!(baked["waypoints"].as_u64().unwrap_or(0) >= 2);
        // gd_transpile round-trips a small script.
        let t = server.dispatch(
            "gd_transpile",
            &serde_json::json!({"source": "extends Node2D\nfunc _ready() -> void:\n\tprint(1)\n"}),
        );
        assert_eq!(t["ok"], true);
        assert!(t["iris"].as_str().unwrap_or("").contains("fn _ready"));
        // render_gpu without the feature reports honestly instead of panicking.
        let g = server.dispatch("render_gpu", &serde_json::json!({}));
        assert!(g.get("ok") == Some(&serde_json::json!(true)) || g.get("error").is_some());
        // import_gltf errors cleanly on missing files.
        let m = server.dispatch("import_gltf", &serde_json::json!({"path": "res://nope.gltf"}));
        assert!(m.get("error").is_some());
    }
    #[test]
    fn server_tools_run_live() {
        let server = McpServer::new();
        let demo = server.dispatch("render_demo", &serde_json::json!({}));
        assert_eq!(demo["ok"], true);
        assert!(demo["coverage"].as_f64().unwrap_or(0.0) > 0.0);
        let phys = server.dispatch("physics3d_step", &serde_json::json!({"steps": 240}));
        assert_eq!(phys["ok"], true);
        assert!((phys["y"].as_f64().unwrap_or(9.0) - 0.5).abs() < 0.05);
        let phys2 = server.dispatch("physics2d_step", &serde_json::json!({"steps": 240}));
        assert_eq!(phys2["ok"], true);
        let tone = server.dispatch("audio_sine", &serde_json::json!({"freq": 440.0, "seconds": 0.1}));
        assert_eq!(tone["samples"], 4410);
        assert!(tone["wav_bytes"].as_u64().unwrap_or(0) > 8000);
    }
    #[test]
    fn live_tree_roundtrip_over_dispatch() {
        let server = McpServer::new();
        let added = server.dispatch(
            "execute",
            &serde_json::json!({"command": "add_node", "name": "S", "class": "Node2D"}),
        );
        assert_eq!(added["ok"], true);
        let id = added["result"]["id"].as_u64().expect("new node id");
        let set = server.dispatch(
            "set_parameter",
            &serde_json::json!({"node": id, "property": "visible", "value": false}),
        );
        assert_eq!(set["ok"], true);
        let inspect = server.dispatch("inspect", &serde_json::json!({}));
        assert!(inspect["node_count"].as_u64().unwrap_or(0) >= 2);
        let names: Vec<String> = inspect["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|n| n["name"].as_str().map(|s| s.to_owned()))
            .collect();
        assert!(names.contains(&"S".to_string()));
        let exported = server.dispatch("export", &serde_json::json!({"format": "json"}));
        assert_eq!(exported["ok"], true);
    }
}

/// Validate a Variant JSON payload for `set_parameter` (shape check only in v0).
pub fn variant_from_json(v: &Value) -> Variant {
    use godot_core::{GodotArray, GodotDictionary};
    match v {
        Value::Null => Variant::Nil,
        Value::Bool(b) => Variant::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Variant::Int(i)
            } else {
                Variant::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => Variant::String(s.clone()),
        Value::Array(a) => {
            let mut arr = GodotArray::new();
            for item in a.iter().map(variant_from_json) {
                let _ = arr.push(item);
            }
            Variant::Array(arr)
        }
        Value::Object(m) => {
            let mut d = GodotDictionary::new();
            for (k, vv) in m {
                let _ = d.set(k.clone(), variant_from_json(vv));
            }
            Variant::Dictionary(d)
        }
    }
}
