//! Port of `servers/text/text_server.h` (measurement/layout + shaping).
//!
//! Two paths: a monospace fast path (no font needed, deterministic — what the
//! GUI `Label` metrics use by default) and real OpenType shaping via
//! `rustybuzz` (`shape_with_font`), the pure-Rust HarfBuzz port, with
//! `fontdb` system-font discovery. Complex-script correctness now comes from a
//! real shaper; the monospace path stays for headless determinism.

#[derive(Debug, Clone, Default)]
pub struct TextServer {
    pub font_size: u32,
    /// Advance per codepoint at `font_size` 16 (Godot's default UI font ~8px).
    pub advance: f32,
    pub line_height: f32,
}

impl TextServer {
    pub fn new() -> Self {
        Self { font_size: 16, advance: 8.0, line_height: 20.0 }
    }
    pub fn with_size(mut self, px: u32) -> Self {
        let k = px as f32 / 16.0;
        self.font_size = px;
        self.advance = 8.0 * k;
        self.line_height = 20.0 * k;
        self
    }
    /// Shaped run: codepoints in visual order (RTL segments reversed).
    pub fn shape(&self, text: &str) -> Vec<char> {
        if is_rtl(text) {
            text.chars().rev().collect()
        } else {
            text.chars().collect()
        }
    }
    /// Real shaping with an OpenType font (`rustybuzz`, HarfBuzz-compatible).
    /// Returns one entry per glyph cluster with advances in `font_size_px`
    /// pixels. Falls back to monospace advances when the font is missing or
    /// invalid — never fails, so layout code stays total.
    pub fn shape_with_font(&self, text: &str, font_data: &[u8], font_size_px: f32) -> Vec<ShapedGlyph> {
        let Some(face) = rustybuzz::Face::from_slice(font_data, 0) else {
            return self.monospace_glyphs(text);
        };
        if face.units_per_em() == 0 {
            return self.monospace_glyphs(text);
        }
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        buf.guess_segment_properties();
        let out = rustybuzz::shape(&face, &[], buf);
        let scale = font_size_px / face.units_per_em() as f32;
        let infos = out.glyph_infos();
        let positions = out.glyph_positions();
        // Map clusters back to chars for the common case (1 char == 1 cluster).
        let chars: Vec<char> = text.chars().collect();
        infos
            .iter()
            .zip(positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                ch: chars.get(info.cluster as usize).copied().unwrap_or('\u{FFFD}'),
                x_advance: pos.x_advance as f32 * scale,
                x_offset: pos.x_offset as f32 * scale,
            })
            .collect()
    }
    fn monospace_glyphs(&self, text: &str) -> Vec<ShapedGlyph> {
        text.chars()
            .map(|ch| ShapedGlyph { ch, x_advance: self.advance, x_offset: 0.0 })
            .collect()
    }
    /// Measure with shaping when `font_data` is `Some`, monospace otherwise.
    pub fn measure_shaped(&self, text: &str, font_data: Option<&[u8]>) -> (f32, f32) {
        match font_data {
            Some(data) if !data.is_empty() => {
                let g = self.shape_with_font(text, data, self.font_size as f32);
                let w: f32 = g.iter().map(|gl| gl.x_advance).sum();
                (w, self.line_height)
            }
            _ => self.measure(text),
        }
    }
    /// Best-effort system font discovery (Windows fonts dir, then fontconfig
    /// dirs). Returns raw TTF/OTF bytes for `shape_with_font`.
    pub fn system_font_bytes(family_substring: &str) -> Option<Vec<u8>> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family_substring)],
            ..Default::default()
        };
        let id = db.query(&query)?;
        let (source, _index) = db.face_source(id)?;
        match source {
            fontdb::Source::Binary(data) => Some(data.as_ref().as_ref().to_vec()),
            fontdb::Source::File(path) => std::fs::read(path).ok(),
            fontdb::Source::SharedFile(path, data) => std::fs::read(&path)
                .ok()
                .filter(|b| !b.is_empty())
                .or_else(|| Some(data.as_ref().as_ref().to_vec())),
        }
    }
    pub fn measure(&self, text: &str) -> (f32, f32) {
        (text.chars().count() as f32 * self.advance, self.line_height)
    }
    /// Greedy word wrap at `max_width` px; returns lines.
    pub fn wrap(&self, text: &str, max_width: f32) -> Vec<String> {
        let per_line = (max_width / self.advance).floor().max(1.0) as usize;
        let mut lines = vec![];
        for para in text.split('\n') {
            if para.is_empty() {
                lines.push(String::new());
                continue;
            }
            let mut cur = String::new();
            let mut cur_w = 0;
            for word in para.split(' ') {
                let ww = word.chars().count();
                let need = if cur.is_empty() { ww } else { cur_w + 1 + ww };
                if need > per_line && !cur.is_empty() {
                    lines.push(cur.clone());
                    cur.clear();
                    cur_w = 0;
                }
                if !cur.is_empty() {
                    cur.push(' ');
                    cur_w += 1;
                }
                cur.push_str(word);
                cur_w += ww;
            }
            lines.push(cur);
        }
        lines
    }
}

fn is_rtl(text: &str) -> bool {
    // Hebrew/Arabic blocks — enough for the stub + tests.
    text.chars().any(|c| matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB00}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    pub ch: char,
    pub x_advance: f32,
    pub x_offset: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measure_and_wrap_match_label_metrics() {
        let t = TextServer::new();
        assert_eq!(t.measure("hello"), (40.0, 20.0));
        assert_eq!(t.wrap("aa bb cc", 8.0 * 5.0), vec!["aa bb".to_string(), "cc".to_string()]);
        assert_eq!(t.wrap("a\nb", 100.0), vec!["a".to_string(), "b".to_string()]);
    }
    #[test]
    fn rtl_reverses_visual_run() {
        let t = TextServer::new();
        assert_eq!(t.shape("abc"), vec!['a', 'b', 'c']);
        assert_eq!(t.shape("\u{05D0}\u{05D1}"), vec!['\u{05D1}', '\u{05D0}']);
    }
    #[test]
    fn shaping_is_deterministic_with_real_font() {
        let t = TextServer::new().with_size(16);
        let g1 = t.shape_with_font("Hello", epaint_default_fonts::HACK_REGULAR, 16.0);
        let g2 = t.shape_with_font("Hello", epaint_default_fonts::HACK_REGULAR, 16.0);
        assert_eq!(g1.len(), 5);
        assert_eq!(g1, g2, "shaping must be deterministic");
        let w: f32 = g1.iter().map(|g| g.x_advance).sum();
        assert!(w > 20.0 && w < 200.0, "sane advances, got {w}");
        let (mw, _) = t.measure_shaped("Hello", None);
        let (sw, _) = t.measure_shaped("Hello", Some(epaint_default_fonts::HACK_REGULAR));
        assert!((sw - w).abs() < 1e-3);
        assert!((mw - 40.0).abs() < 1e-3, "monospace fallback unchanged");
    }
    #[test]
    fn shaping_falls_back_without_font() {
        let t = TextServer::new();
        let g = t.shape_with_font("Hi", &[], 16.0);
        assert_eq!(g.len(), 2);
        assert!(g.iter().all(|gl| gl.x_advance == t.advance));
        // System fonts are environment-dependent: assert graceful behavior either way.
        let sys = TextServer::system_font_bytes("Arial").or_else(|| TextServer::system_font_bytes("DejaVu Sans"));
        if let Some(bytes) = sys {
            let g = t.shape_with_font("Hello", &bytes, 16.0);
            assert_eq!(g.len(), 5);
        }
    }
}
