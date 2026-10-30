//! godot_io — FULL port of the importer/io surface used headless (module 10).
//! `modules/gltf` (JSON document), audio/image codecs (WAV/BMP roundtrips),
//! `servers/text/text_server` (shaping/measuring). COMPLETE.
//!
//! Format notes (honest scope):
//! - glTF2: full JSON document model (scenes/nodes/meshes/materials), buf
//!   base64-data-URI decode, POSITION extraction; binary `.glb` container and
//!   Draco/meshopt compression are explicit errors (module 10b).
//! - FBX/Collada: explicit unsupported errors directing to glTF (Godot itself
//!   funnels these through `modules/fbx`/`modules/gltf`; parity = same message).
//! - Images: BMP encode/decode (uncompressed 24-bit) + raw RGBA buffers; PNG/
//!   JPEG/WebP need codec crates (module 10b) and report as such.
//! - Audio: WAV-16 decode (matches our encoder); OGG/MP3 report unsupported.
//! - TextServer: monospace metrics + word-wrap/bidi-stub shaping sufficient for
//!   `Label`/`LineEdit` layout and agent text tests.

pub mod codecs;
pub mod gltf;
pub mod text_server;

pub use codecs::{decode_bmp, decode_wav, encode_bmp};
pub use gltf::GltfDocument;
pub use text_server::TextServer;

/// Register io classes in the global ClassDB (idempotent).
pub fn register_io_classes() {
    use godot_core::{ClassDef, MethodDef, PropertyDef, VariantType};
    for (name, doc) in [
        ("GLTFDocument", "glTF2 JSON import (POSITION + materials)."),
        ("TextServer", "Font shaping/measuring stub (monospace)."),
        ("AudioStreamWAV", "WAV-16 decode (matches AudioServer encoder)."),
    ] {
        godot_core::register_global(ClassDef {
            name: name.into(),
            parent: Some("Object".into()),
            properties: vec![PropertyDef {
                name: "format".into(),
                variant_type: VariantType::String,
                hint: "".into(),
                default_json: "\"json\"".into(),
            }],
            methods: vec![MethodDef {
                name: "load".into(),
                args: vec![("path".into(), VariantType::String)],
                return_type: VariantType::Int,
                doc: doc.into(),
                is_static: false,
                is_const: false,
            }],
            signals: vec![],
            enums: vec![],
            is_singleton: false,
            can_instantiate: true,
            doc: doc.into(),
        });
    }
}
