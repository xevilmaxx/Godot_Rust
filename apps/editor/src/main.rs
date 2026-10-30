//! godot_rs_editor — headless editor shell + engine launcher.
//! Commands mirror SolidCraft UX (agents/models/mcp) plus engine verbs
//! (launch/doc/demos). The windowed egui front-end lands in module 04b;
//! until then `launch` runs the full main loop headlessly.

#[cfg(feature = "gui")]
mod gui;

use godot_ai::{AcpAgent, McpServer, ModelRegistry};
use godot_core::ClassDb;
use godot_scene::SceneTree;
use godot_script_iris as iris;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match cmd {
        "agents" => {
            println!("Installed ACP agents (SolidCraft AI → Refresh equivalent):\n");
            for a in AcpAgent::all_builtin() {
                let status = if a.is_installed() { "READY" } else { "missing" };
                println!("- {:<12} [{:>7}]  run: {}  | install: {}", a.label, status, a.spawn.join(" "), a.install_cmd);
            }
            println!("\nNext: AI → Connect <agent>, then pick model with `models`.");
        }
        "models" => {
            let reg = ModelRegistry::with_builtin_offline();
            println!("Model dropdown (v0 offline; /models fetch in module 02):");
            for m in &reg.models {
                println!("- {}  ({})", m.id, m.label);
            }
            println!("\nPresets: Ollama http://localhost:11434/v1, LM Studio http://localhost:1234/v1, OpenAI, OpenCode Go.");
        }
        "mcp" => {
            let server = McpServer::new();
            if let Err(e) = server.serve_stdio() {
                eprintln!("mcp error: {e}");
                std::process::exit(1);
            }
        }
        "tools" => {
            let server = McpServer::new();
            println!("{}", serde_json::to_string_pretty(&server.tools).unwrap());
        }
        "check" => {
            let path = args.get(2).cloned().unwrap_or_else(|| "demo.iris".into());
            let src = std::fs::read_to_string(&path).unwrap_or_else(|_| {
                eprintln!("cannot read {path}; using inline demo");
                "node Demo : Node2D {\nprop speed: float = 200.0\nfn _process(dt: float) {\nprint speed\n}\n}\n".into()
            });
            match iris::parse(&src) {
                Ok(f) => {
                    let diags = iris::check(&f);
                    println!("parsed {} (base {}), {} props, {} fns", f.node_class, f.base_class, f.props.len(), f.fns.len());
                    println!("{}", serde_json::to_string_pretty(&diags).unwrap());
                }
                Err(e) => {
                    eprintln!("parse error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "run" => {
            let path = args.get(2).cloned().unwrap_or_else(|| "demo.iris".into());
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            match iris::parse(&src) {
                Ok(f) => {
                    let mut interp = iris::Interpreter::new();
                    for func in &f.fns {
                        interp.run(&func.body);
                    }
                    for line in &interp.output {
                        println!("{line}");
                    }
                }
                Err(e) => {
                    eprintln!("parse error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "new-scene" => {
            let mut tree = SceneTree::new();
            let node = godot_scene::Node::new("MySprite");
            let id = tree.add_node(node, None);
            tree.process_frame(1.0 / 60.0);
            println!("scene: root={:?} added={:?} nodes={} frame={}", tree.root_id(), id, tree.node_count(), tree.frame);
            let _ = ClassDb::new();
            println!("classdb: {}", godot_core::global_db().len());
        }
        "scene-demo" => {
            // Module-02 end-to-end: tree + groups + timers + Iris + print_tree.
            godot_scene::register_scene_classes();
            let mut tree = SceneTree::new();
            let a = tree.add_base(godot_scene::Node::new("Player"), None).expect("add Player");
            let mut sprite = godot_scene::Node2D::new("Sprite");
            sprite.set_position(godot_core::Vector2::new(10.0, 0.0));
            let b = tree.add_2d(sprite, Some(a)).expect("add Sprite");
            tree.add_to_group(b, "renderables").expect("group");
            tree.attach_script(
                b,
                "node Sprite : Node2D {\nfn _ready() {\nprint booted\n}\nfn _process(dt: float) {\nprint tick\n}\n}\n",
            )
            .expect("attach");
            tree.enable_process(b, true, false).expect("enable");
            let timer = tree.create_timer(1.0, true, true);
            tree.process_frame(1.0 / 60.0);
            tree.process_frame(1.0 / 60.0);
            println!("{}", tree.print_tree_pretty());
            println!("timer {timer} left: {:.3}", tree.timer_time_left(timer).unwrap_or(-1.0));
            println!("group renderables: {:?}", tree.get_nodes_in_group("renderables"));
            println!("script output: {:?}", tree.script_output(b));
            println!("inspect nodes: {}", tree.inspect_summary()["node_count"]);
            let _ = a;
        }
        "servers-demo" => {
            // Module-03 end-to-end: render a box, drop a ball, mix a tone.
            godot_servers::register_server_classes();
            let mut rendering = godot_servers::RenderingServer::default();
            let vp = rendering.viewport_create(64, 64, godot_core::Color::BLACK);
            let cam = rendering.camera_create(70.0, 0.05, 100.0);
            rendering.camera_set_transform(
                cam,
                godot_core::Transform3D::from_translation(godot_core::Vector3::new(0.0, 0.0, 3.0)),
            );
            rendering.viewport_set_camera(vp, Some(cam));
            let mesh = rendering.mesh_create_box(godot_core::Vector3::new(0.5, 0.5, 0.5));
            let mat = rendering.material_create(godot_core::Color::RED);
            let _inst = rendering.instance_create(Some(mesh), Some(mat));
            rendering.render();
            println!("render: coverage={:.2} draws={} tris={}", rendering.coverage(vp).unwrap_or(-1.0), rendering.stats.draw_calls, rendering.stats.triangles);

            let mut physics = godot_servers::PhysicsServer3D::new();
            let space = physics.space_create(godot_core::Vector3::new(0.0, -9.8, 0.0));
            let ball = physics
                .body_create(space, godot_servers::physics3d::BodyMode3D::Rigid, 1.0)
                .expect("ball");
            physics.body_add_shape(space, ball, godot_servers::physics3d::Shape3D::Sphere { radius: 0.5 });
            physics.body_set_position(space, ball, godot_core::Vector3::new(0.0, 5.0, 0.0));
            for _ in 0..240 {
                physics.space_step(space, 1.0 / 60.0);
            }
            println!(
                "physics: y={:.3} contacts={}",
                physics.body_get_position(space, ball).map(|v| v.y).unwrap_or(-1.0),
                physics.contact_count(space)
            );

            let mut audio = godot_servers::AudioServer::new();
            let p = audio.player_create(godot_servers::audio::Stream::Sine { freq: 440.0 }, 0);
            audio.play(p, 0.0);
            let pcm = audio.render_pcm(0.1);
            let zc = pcm.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
            let wav = godot_servers::AudioServer::encode_wav(&pcm);
            println!("audio: samples={} zero_crossings={} wav_bytes={}", pcm.len(), zc, wav.len());
        }
        "gui" => {
            #[cfg(feature = "gui")]
            {
                if let Err(e) = gui::run_gui() {
                    eprintln!("gui error: {e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(feature = "gui"))]
            {
                eprintln!("rebuild with the gui feature: cargo run -p editor --features gui -- gui");
                std::process::exit(2);
            }
        }
        "launch" => {
            // Full engine boot: settings + tree + servers + Iris, N frames.
            let frames: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(120);
            godot_scene::register_scene_classes();
            godot_servers::register_server_classes();
            godot_host::register_host_classes();
            godot_scene_ext::register_scene_ext_classes();
            godot_net::register_net_classes();
            godot_nav::register_nav_classes();
            godot_io::register_io_classes();
            let mut engine = godot_host::Engine::new(godot_host::ProjectSettings::new());
            println!("godot_rs {} — {} ({})", godot_core::ENGINE_VERSION, std::env::consts::OS, engine.display.name());
            println!("viewport {:?}, physics {} Hz", engine.display.window_size(), engine.settings.physics_ticks());
            // Demo scene: a scripted mover + a rendered box + a falling ball + a tone.
            let mover = engine.tree.add_2d(godot_scene::Node2D::new("Mover"), None).expect("mover");
            engine.tree.attach_script(mover,
                "node Mover : Node2D {\nprop speed: float = 200.0\nfn _process(dt: float) {\nposition.x += speed * dt\n}\n}\n").expect("script");
            engine.tree.enable_process(mover, true, false).expect("enable");
            let mesh = engine.rendering.mesh_create_box(godot_core::Vector3::new(0.5, 0.5, 0.5));
            let mat = engine.rendering.material_create(godot_core::Color::RED);
            let vp = engine.rendering.viewport_create(64, 64, godot_core::Color::BLACK);
            let cam = engine.rendering.camera_create(70.0, 0.05, 100.0);
            engine.rendering.camera_set_transform(cam, godot_core::Transform3D::from_translation(godot_core::Vector3::new(0.0, 0.0, 3.0)));
            engine.rendering.viewport_set_camera(vp, Some(cam));
            let _inst = engine.rendering.instance_create(Some(mesh), Some(mat));
            let space = engine.physics3d.space_create(godot_core::Vector3::new(0.0, -9.8, 0.0));
            let ball = engine.physics3d.body_create(space, godot_servers::physics3d::BodyMode3D::Rigid, 1.0).expect("ball");
            engine.physics3d.body_add_shape(space, ball, godot_servers::physics3d::Shape3D::Sphere { radius: 0.5 });
            engine.physics3d.body_set_position(space, ball, godot_core::Vector3::new(0.0, 5.0, 0.0));
            let tone = engine.audio.player_create(godot_servers::audio::Stream::Sine { freq: 220.0 }, 0);
            engine.audio.play(tone, 0.0);
            let dt = 1.0 / 60.0;
            for f in 1..=frames.min(3600) {
                engine.tree.process_frame(dt);
                engine.physics3d.space_step(space, dt);
                engine.audio.tick(dt);
                engine.rendering.render();
                engine.frames += 1;
                if f % 60 == 0 {
                    let x = engine.tree.get_prop(mover, "position").ok();
                    let y = engine.physics3d.body_get_position(space, ball).map(|v| v.y).unwrap_or(f32::NAN);
                    println!("frame {f}: mover={x:?} ball_y={y:.3} draws={} audio_t={:.2}s",
                        engine.rendering.stats.draw_calls, engine.audio.time);
                }
            }
            println!("done: {} frames, {} nodes, classdb={} classes",
                engine.frames, engine.tree.node_count(), godot_core::global_db().len());
        }
        "doc" => {
            // Dump machine-readable API docs for agents and humans.
            let outdir = args.get(2).cloned().unwrap_or_else(|| "docs/api".into());
            let _ = std::fs::create_dir_all(&outdir);
            godot_scene::register_scene_classes();
            godot_servers::register_server_classes();
            godot_host::register_host_classes();
            godot_scene_ext::register_scene_ext_classes();
            godot_net::register_net_classes();
            godot_nav::register_nav_classes();
            godot_io::register_io_classes();
            let api = godot_core::global_db().to_json();
            std::fs::write(format!("{outdir}/classdb.json"), &api).expect("write classdb");
            let summary = godot_core::global_db().to_ai_summary();
            std::fs::write(format!("{outdir}/classes.txt"), &summary).expect("write summary");
            let src = std::fs::read_to_string("demo.iris").unwrap_or_else(|_| "node Demo : Node {\nfn main() {\nprint 1\n}\n}\n".into());
            if let Ok(f) = iris::parse(&src) {
                let doc = iris::doc_json(&f);
                std::fs::write(format!("{outdir}/iris.json"), serde_json::to_string_pretty(&doc).unwrap()).expect("write iris");
            }
            let mcp = McpServer::new();
            std::fs::write(format!("{outdir}/mcp_tools.json"), serde_json::to_string_pretty(&mcp.tools).unwrap()).expect("write tools");
            println!("docs written to {outdir}/ (classdb.json, classes.txt, iris.json, mcp_tools.json)");
        }
        _ => {
            println!("godot_rs_editor {} — Rust port of Godot 4.7 (all modules)\n", godot_core::ENGINE_VERSION);
            println!("  gui        open the Godot-style GUI editor (needs --features gui)");
            println!("  launch [n] run the engine headless for n frames (default 120)");
            println!("  doc [dir]  dump ClassDB + Iris + MCP docs for agents");
            println!("  agents     list ACP agents + install cmds (SolidCraft-style)");
            println!("  models     list model dropdown presets");
            println!("  mcp        serve MCP over stdio (for Claude Code / MCP clients)");
            println!("  tools      print MCP tool schemas");
            println!("  check [f]  iris check — parse + AI-readable diagnostics");
            println!("  run [f]    iris run — execute script");
            println!("  new-scene  smoke-test SceneTree + ClassDB");
            println!("  scene-demo end-to-end scene demo (tree+groups+timers+Iris)");
            println!("  servers-demo end-to-end servers demo (render+physics+audio)");
        }
    }
}
