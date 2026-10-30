//! Port of `modules/gltf/gltf_document.h` (JSON document subset) + explicit
//! FBX/Collada redirection. Buffer data-URI decoding included.

use godot_core::{GodotError, Vector3};
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct GltfMesh {
    pub name: String,
    pub positions: Vec<Vector3>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct GltfMaterial {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

#[derive(Debug, Clone, Default)]
pub struct GltfNode {
    pub name: String,
    pub translation: Vector3,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Default)]
pub struct GltfDocument {
    pub scenes: Vec<Vec<usize>>,
    pub nodes: Vec<GltfNode>,
    pub meshes: Vec<GltfMesh>,
    pub materials: Vec<GltfMaterial>,
    pub generator: String,
}

impl GltfDocument {
    /// Parse a `.gltf` JSON document (not `.glb`; see `from_glb`/`from_file`).
    pub fn from_json(text: &str) -> Result<Self, GodotError> {
        Self::from_json_impl(text, None)
    }

    fn from_json_impl(
        text: &str,
        external: Option<&dyn Fn(usize, &str) -> Result<Vec<u8>, GodotError>>,
    ) -> Result<Self, GodotError> {
        let v: Value = serde_json::from_str(text).map_err(|e| GodotError::Parse(format!("glTF JSON: {e}")))?;
        if v.get("asset").is_none() {
            return Err(GodotError::Parse("glTF: missing `asset`".into()));
        }
        let mut doc = Self::default();
        doc.generator = v["asset"]["generator"].as_str().unwrap_or("").to_owned();
        // Materials.
        for m in v["materials"].as_array().cloned().unwrap_or_default() {
            let pbr = &m["pbrMetallicRoughness"];
            let bc = pbr["baseColorFactor"].as_array().cloned().unwrap_or_default();
            let f = |i: usize| bc.get(i).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
            doc.materials.push(GltfMaterial {
                name: m["name"].as_str().unwrap_or("").into(),
                base_color: [f(0), f(1), f(2), if bc.len() >= 4 { f(3) } else { 1.0 }],
                metallic: pbr["metallicFactor"].as_f64().unwrap_or(1.0) as f32,
                roughness: pbr["roughnessFactor"].as_f64().unwrap_or(1.0) as f32,
            });
        }
        // Buffers: data-URI (embedded), external `.bin` resolved by the
        // caller via `glb_bin`/`external`, or `.glb` BIN chunk.
        let mut buffers: Vec<Vec<u8>> = vec![];
        for (bi, b) in v["buffers"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            let uri = b["uri"].as_str().unwrap_or("");
            if uri.starts_with("data:") {
                buffers.push(decode_data_uri(uri)?);
            } else if let Some(external) = external {
                // External .bin resolved relative to the .gltf file by from_file.
                let data = external(bi, uri)?;
                buffers.push(data);
            } else {
                return Err(GodotError::Parse(format!(
                    "external buffer `{uri}` needs from_file (or embed a data: URI)"
                )));
            }
        }
        // BufferViews + accessors helpers.
        let views = v["bufferViews"].as_array().cloned().unwrap_or_default();
        let accessors = v["accessors"].as_array().cloned().unwrap_or_default();
        let components = |acc_idx: usize| -> Result<usize, GodotError> {
            let acc = accessors.get(acc_idx).ok_or_else(|| GodotError::Parse(format!("bad accessor {acc_idx}")))?;
            match acc["type"].as_str().unwrap_or("SCALAR") {
                "SCALAR" => Ok(1),
                "VEC2" => Ok(2),
                "VEC3" => Ok(3),
                "VEC4" => Ok(4),
                t => Err(GodotError::Parse(format!("accessor type {t} unsupported"))),
            }
        };
        let read_f32 = |acc_idx: usize| -> Result<Vec<f32>, GodotError> {
            let acc = accessors.get(acc_idx).ok_or_else(|| GodotError::Parse(format!("bad accessor {acc_idx}")))?;
            let view_idx = acc["bufferView"].as_u64().ok_or_else(|| GodotError::Parse("accessor without bufferView (sparse unsupported)".into()))? as usize;
            let view = views.get(view_idx).ok_or_else(|| GodotError::Parse("bad bufferView".into()))?;
            let buf_idx = view["buffer"].as_u64().unwrap_or(0) as usize;
            let byte_offset = view["byteOffset"].as_u64().unwrap_or(0) as usize + acc["byteOffset"].as_u64().unwrap_or(0) as usize;
            let count = acc["count"].as_u64().unwrap_or(0) as usize;
            let comp = acc["componentType"].as_u64().unwrap_or(5126);
            if comp != 5126 {
                return Err(GodotError::Parse(format!("only FLOAT accessors in v1 (got {comp})")));
            }
            let n = count * components(acc_idx)?;
            let buf = buffers.get(buf_idx).ok_or_else(|| GodotError::Parse("bad buffer".into()))?;
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let o = byte_offset + i * 4;
                if o + 4 > buf.len() {
                    return Err(GodotError::Parse("accessor out of range".into()));
                }
                out.push(f32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]));
            }
            Ok(out)
        };
        let read_u32 = |acc_idx: usize| -> Result<Vec<u32>, GodotError> {
            let acc = accessors.get(acc_idx).ok_or_else(|| GodotError::Parse(format!("bad accessor {acc_idx}")))?;
            let view_idx = acc["bufferView"].as_u64().ok_or_else(|| GodotError::Parse("accessor without bufferView".into()))? as usize;
            let view = views.get(view_idx).ok_or_else(|| GodotError::Parse("bad bufferView".into()))?;
            let buf_idx = view["buffer"].as_u64().unwrap_or(0) as usize;
            let byte_offset = view["byteOffset"].as_u64().unwrap_or(0) as usize + acc["byteOffset"].as_u64().unwrap_or(0) as usize;
            let count = acc["count"].as_u64().unwrap_or(0) as usize;
            let comp = acc["componentType"].as_u64().unwrap_or(5125);
            let buf = buffers.get(buf_idx).ok_or_else(|| GodotError::Parse("bad buffer".into()))?;
            let mut out = Vec::with_capacity(count);
            for i in 0..count {
                match comp {
                    5123 => {
                        // UNSIGNED_SHORT
                        let o = byte_offset + i * 2;
                        if o + 2 > buf.len() {
                            return Err(GodotError::Parse("accessor out of range".into()));
                        }
                        out.push(u16::from_le_bytes([buf[o], buf[o + 1]]) as u32);
                    }
                    5125 => {
                        let o = byte_offset + i * 4;
                        if o + 4 > buf.len() {
                            return Err(GodotError::Parse("accessor out of range".into()));
                        }
                        out.push(u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]));
                    }
                    _ => return Err(GodotError::Parse(format!("index component {comp} unsupported in v1"))),
                }
            }
            Ok(out)
        };
        // Meshes (POSITION + indices + first material).
        for m in v["meshes"].as_array().cloned().unwrap_or_default() {
            let prims = m["primitives"].as_array().cloned().unwrap_or_default();
            let prim = prims.first().ok_or_else(|| GodotError::Parse("mesh without primitives".into()))?;
            let pos_acc = prim["attributes"]["POSITION"].as_u64().ok_or_else(|| GodotError::Parse("primitive without POSITION".into()))? as usize;
            let floats = read_f32(pos_acc)?;
            if floats.len() % 3 != 0 {
                return Err(GodotError::Parse("POSITION count not multiple of 3".into()));
            }
            let positions: Vec<Vector3> =
                floats.chunks_exact(3).map(|c| Vector3::new(c[0], c[1], c[2])).collect();
            let indices = match prim.get("indices").and_then(|i| i.as_u64()) {
                Some(a) => read_u32(a as usize)?,
                None => (0..positions.len() as u32).collect(),
            };
            doc.meshes.push(GltfMesh {
                name: m["name"].as_str().unwrap_or("").into(),
                positions,
                indices,
                material: prim.get("material").and_then(|x| x.as_u64()).map(|i| i as usize),
            });
        }
        // Nodes + scenes.
        for n in v["nodes"].as_array().cloned().unwrap_or_default() {
            let t = n["translation"].as_array().cloned().unwrap_or_default();
            let f = |i: usize, d: f32| t.get(i).and_then(|x| x.as_f64()).unwrap_or(d as f64) as f32;
            doc.nodes.push(GltfNode {
                name: n["name"].as_str().unwrap_or("").into(),
                translation: Vector3::new(f(0, 0.0), f(1, 0.0), f(2, 0.0)),
                mesh: n.get("mesh").and_then(|x| x.as_u64()).map(|i| i as usize),
                children: n["children"].as_array().cloned().unwrap_or_default().iter().filter_map(|c| c.as_u64().map(|i| i as usize)).collect(),
            });
        }
        for s in v["scenes"].as_array().cloned().unwrap_or_default() {
            doc.scenes.push(s["nodes"].as_array().cloned().unwrap_or_default().iter().filter_map(|c| c.as_u64().map(|i| i as usize)).collect());
        }
        Ok(doc)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, GodotError> {
        if bytes.len() >= 4 && &bytes[0..4] == b"glTF" {
            return Self::from_glb(bytes);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| GodotError::Parse("not UTF-8 JSON".into()))?;
        Self::from_json(text)
    }

    /// `.glb` binary container: header + JSON chunk + BIN chunk (no extensions).
    pub fn from_glb(bytes: &[u8]) -> Result<Self, GodotError> {
        let u32le = |o: usize| -> Result<u32, GodotError> {
            bytes.get(o..o + 4).ok_or_else(|| GodotError::Parse("truncated .glb".into())).map(|w| {
                u32::from_le_bytes([w[0], w[1], w[2], w[3]])
            })
        };
        if bytes.len() < 12 || &bytes[0..4] != b"glTF" {
            return Err(GodotError::Parse("not a .glb file".into()));
        }
        if u32le(4)? != 2 {
            return Err(GodotError::Parse("only glTF 2.0 .glb containers".into()));
        }
        let total = u32le(8)? as usize;
        if bytes.len() < total {
            return Err(GodotError::Parse("truncated .glb".into()));
        }
        let mut pos = 12;
        let mut json: Option<&str> = None;
        let mut bin: Option<&[u8]> = None;
        while pos + 8 <= total {
            let len = u32le(pos)? as usize;
            let kind = &bytes[pos + 4..pos + 8];
            let start = pos + 8;
            let end = start + len;
            if end > total {
                return Err(GodotError::Parse("truncated .glb chunk".into()));
            }
            if kind == b"JSON" {
                json = Some(std::str::from_utf8(&bytes[start..end]).map_err(|_| GodotError::Parse("glb JSON not UTF-8".into()))?);
            } else if kind == b"BIN\x00" {
                bin = Some(&bytes[start..end]);
            }
            pos = end;
        }
        let text = json.ok_or_else(|| GodotError::Parse(".glb without JSON chunk".into()))?;
        match bin {
            // Buffer 0 with an empty/missing URI resolves to the BIN chunk.
            Some(b) => Self::from_json_impl(text, Some(&|bi: usize, uri: &str| {
                if bi == 0 && (uri.is_empty() || !uri.starts_with("data:")) {
                    Ok(b.to_vec())
                } else if uri.starts_with("data:") {
                    decode_data_uri(uri)
                } else {
                    Err(GodotError::Parse(format!("external buffer `{uri}` needs from_file")))
                }
            })),
            None => Self::from_json(text),
        }
    }

    /// Load `.gltf` (JSON, external `.bin` resolved next to the file) or `.glb`.
    pub fn from_file(path: &str) -> Result<Self, GodotError> {
        let bytes = std::fs::read(path).map_err(|e| GodotError::Io(format!("{path}: {e}")))?;
        if bytes.len() >= 4 && &bytes[0..4] == b"glTF" {
            return Self::from_glb(&bytes);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| GodotError::Parse("not UTF-8 JSON".into()))?;
        let dir = std::path::Path::new(path).parent();
        Self::from_json_impl(text, Some(&|bi: usize, uri: &str| {
            let _ = bi;
            if uri.starts_with("data:") {
                return decode_data_uri(uri);
            }
            let full = match dir {
                Some(d) => d.join(uri),
                None => std::path::PathBuf::from(uri),
            };
            std::fs::read(&full).map_err(|e| GodotError::Io(format!("{}: {e}", full.display())))
        }))
    }

    pub fn fbx_stub() -> GodotError {
        GodotError::Unimplemented("FBX: convert to glTF with fbx2gltf, then import (same as Godot's modules/fbx path)".into())
    }

    pub fn mesh_vertex_count(&self, mesh: usize) -> Option<usize> {
        self.meshes.get(mesh).map(|m| m.positions.len())
    }
}

fn decode_data_uri(uri: &str) -> Result<Vec<u8>, GodotError> {
    let rest = uri.strip_prefix("data:").ok_or_else(|| GodotError::Parse("only embedded data: URIs in v1 (external .bin in 10b)".into()))?;
    let comma = rest.find(',').ok_or_else(|| GodotError::Parse("malformed data URI".into()))?;
    let (meta, data) = (&rest[..comma], &rest[comma + 1..]);
    if !meta.contains(";base64") {
        return Err(GodotError::Parse("only base64 data URIs in v1".into()));
    }
    base64_decode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, GodotError> {
    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let clean: Vec<u8> = s.bytes().filter(|&c| c != b'=' && !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let mut n = 0u32;
        let mut bits = 0;
        for &c in chunk {
            let v = table(c).ok_or_else(|| GodotError::Parse("bad base64".into()))? as u32;
            n = (n << 6) | v;
            bits += 6;
        }
        // Emit (bits/8) bytes, padding-aware via chunk length.
        let bytes = chunk.len() * 6 / 8;
        for i in 0..bytes {
            let shift = bits - 8 * (i + 1);
            out.push(((n >> shift) & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tri_bin() -> Vec<u8> {
        // One triangle: positions (3×VEC3 float) + uint16 indices.
        let mut bin = vec![];
        for v in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for v in [0u16, 1, 2] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        bin
    }
    fn tri_json_buffers(buffers_json: &str) -> String {
        format!(
            r#"{{"asset":{{"version":"2.0","generator":"test"}},"scenes":[{{"nodes":[0]}}],"nodes":[{{"name":"Tri","translation":[0,0,0],"mesh":0}}],"meshes":[{{"name":"Tri","primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"material":0}}]}}],"materials":[{{"name":"Red","pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1]}}}}],"buffers":[{buffers_json}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
        )
    }
    fn tri_doc() -> String {
        let b64 = base64_encode(&tri_bin());
        tri_json_buffers(&format!(r#"{{"byteLength":42,"uri":"data:application/octet-stream;base64,{b64}"}}"#))
    }
    fn glb_bytes(json: &str, bin: &[u8]) -> Vec<u8> {
        fn pad(mut v: Vec<u8>, byte: u8) -> Vec<u8> {
            while v.len() % 4 != 0 {
                v.push(byte);
            }
            v
        }
        let json = pad(json.as_bytes().to_vec(), b' ');
        let bin = pad(bin.to_vec(), 0);
        let mut out = vec![];
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\x00");
        out.extend_from_slice(&bin);
        out
    }
    pub fn base64_encode(bytes: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let (a, b, d) = (c[0] as u32, *c.get(1).unwrap_or(&0) as u32, *c.get(2).unwrap_or(&0) as u32);
            let n = (a << 16) | (b << 8) | d;
            out.push(T[((n >> 18) & 63) as usize] as char);
            out.push(T[((n >> 12) & 63) as usize] as char);
            out.push(if c.len() > 1 { T[((n >> 6) & 63) as usize] as char } else { '=' });
            out.push(if c.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
        }
        out
    }
    #[test]
    fn triangle_imports_positions_and_material() {
        let doc = GltfDocument::from_json(&tri_doc()).unwrap();
        assert_eq!(doc.meshes.len(), 1);
        assert_eq!(doc.meshes[0].positions.len(), 3);
        assert_eq!(doc.meshes[0].positions[1], Vector3::new(1.0, 0.0, 0.0));
        assert_eq!(doc.meshes[0].indices, vec![0, 1, 2]);
        assert_eq!(doc.materials[0].base_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.nodes[0].name, "Tri");
    }
    #[test]
    fn glb_and_missing_asset_error_helpfully() {
        assert!(GltfDocument::from_slice(b"glTFfake-binary").is_err());
        assert!(GltfDocument::from_json(r#"{"scene":0}"#).is_err());
    }
    #[test]
    fn glb_container_imports_same_triangle() {
        let json = tri_json_buffers(r#"{"byteLength":42}"#);
        let glb = glb_bytes(&json, &tri_bin());
        let doc = GltfDocument::from_glb(&glb).unwrap();
        assert_eq!(doc.meshes[0].positions.len(), 3);
        assert_eq!(doc.meshes[0].positions[2], Vector3::new(0.0, 1.0, 0.0));
        // from_slice sniffs the magic too.
        let doc2 = GltfDocument::from_slice(&glb).unwrap();
        assert_eq!(doc2.meshes[0].indices, vec![0, 1, 2]);
        // Truncated glb errors instead of panicking.
        assert!(GltfDocument::from_glb(&glb[..20].to_vec()).is_err());
    }
    #[test]
    fn external_bin_resolves_next_to_file() {
        let dir = std::env::temp_dir().join("godot_rs_gltf_test");
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.join("tri_ext_test");
        let gltf_path = base.with_extension("gltf");
        let bin_path = base.with_extension("bin");
        std::fs::write(&bin_path, tri_bin()).unwrap();
        let json = tri_json_buffers(r#"{"byteLength":42,"uri":"tri_ext_test.bin"}"#);
        std::fs::write(&gltf_path, &json).unwrap();
        let doc = GltfDocument::from_file(gltf_path.to_str().unwrap()).unwrap();
        assert_eq!(doc.meshes[0].positions.len(), 3);
        let _ = std::fs::remove_file(&gltf_path);
        let _ = std::fs::remove_file(&bin_path);
    }
}
