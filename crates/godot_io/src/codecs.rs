//! Minimal codecs: WAV-16 decode (matches `AudioServer::encode_wav`) and
//! uncompressed 24-bit BMP encode/decode. OGG/MP3/PNG report unsupported with
//! the same message shape as Godot's codec registry.

use godot_core::{Color, GodotError};

#[derive(Debug, Clone, PartialEq)]
pub struct WavData {
    pub samples: Vec<f32>,
    pub rate: u32,
}

pub fn decode_wav(bytes: &[u8]) -> Result<WavData, GodotError> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(GodotError::Parse("not a WAV file".into()));
    }
    let u16 = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32 = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    if &bytes[12..16] != b"fmt " || u32(16) != 16 || u16(20) != 1 {
        return Err(GodotError::Parse("only PCM WAV in v1".into()));
    }
    let channels = u16(22) as usize;
    let rate = u32(24);
    if channels != 1 || u16(34) != 16 {
        return Err(GodotError::Parse("only mono 16-bit WAV in v1".into()));
    }
    // Find the data chunk (assume canonical 44-byte header from our encoder).
    if &bytes[36..40] != b"data" {
        return Err(GodotError::Parse("WAV without data chunk".into()));
    }
    let n = u32(40) as usize / 2;
    if bytes.len() < 44 + n * 2 {
        return Err(GodotError::Parse("WAV truncated".into()));
    }
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let v = i16::from_le_bytes([bytes[44 + i * 2], bytes[44 + i * 2 + 1]]);
        samples.push(v as f32 / 32767.0);
    }
    Ok(WavData { samples, rate })
}

/// Decode PNG/JPEG (and any `image`-crate format) to top-down RGB pixels.
/// MP3 stays a documented stub (C decoder dependency); use WAV/OGG.
pub fn decode_image(bytes: &[u8]) -> Result<BmpImage, GodotError> {
    let img = image::load_from_memory(bytes).map_err(|e| GodotError::Parse(format!("image decode: {e}")))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let pixels = rgba
        .pixels()
        .map(|p| Color::rgb(p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0))
        .collect();
    Ok(BmpImage { width: w, height: h, pixels })
}

/// Decode OGG Vorbis to mono f32 samples (pure-Rust `lewton`).
/// Multi-channel streams are downmixed by averaging.
pub fn decode_ogg(bytes: &[u8]) -> Result<WavData, GodotError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut stream = lewton::inside_ogg::OggStreamReader::new(cursor)
        .map_err(|e| GodotError::Parse(format!("not an OGG Vorbis stream: {e:?}")))?;
    let channels = stream.ident_hdr.audio_channels as usize;
    let rate = stream.ident_hdr.audio_sample_rate;
    if channels == 0 {
        return Err(GodotError::Parse("OGG with no channels".into()));
    }
    let mut mono: Vec<f32> = vec![];
    while let Some(packet) = stream
        .read_dec_packet_itl()
        .map_err(|e| GodotError::Parse(format!("OGG decode: {e:?}")))?
    {
        for frame in packet.chunks(channels) {
            let sum: i32 = frame.iter().copied().map(i32::from).sum();
            mono.push(sum as f32 / (channels as f32 * i16::MAX as f32));
        }
        if mono.len() > 64 * 1024 * 1024 {
            return Err(GodotError::Parse("OGG too large (>64M samples)".into()));
        }
    }
    Ok(WavData { samples: mono, rate })
}

#[derive(Debug, Clone, PartialEq)]
pub struct BmpImage {
    pub width: u32,
    pub height: u32,
    /// Top-down rows.
    pub pixels: Vec<Color>,
}

pub fn encode_bmp(img: &BmpImage) -> Vec<u8> {
    let row_stride = ((img.width * 3 + 3) / 4) * 4;
    let mut out = Vec::with_capacity((54 + row_stride * img.height) as usize);
    let w16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let w32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    out.extend_from_slice(b"BM");
    w32(&mut out, 54 + row_stride * img.height);
    w16(&mut out, 0);
    w16(&mut out, 0);
    w32(&mut out, 54);
    w32(&mut out, 40);
    w32(&mut out, img.width);
    w32(&mut out, img.height);
    w16(&mut out, 1);
    w16(&mut out, 24);
    w32(&mut out, 0);
    w32(&mut out, row_stride * img.height);
    w32(&mut out, 2835);
    w32(&mut out, 2835);
    w32(&mut out, 0);
    w32(&mut out, 0);
    // BMP is bottom-up, BGR.
    for y in (0..img.height).rev() {
        for x in 0..img.width {
            let p = img.pixels[(y * img.width + x) as usize];
            let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            out.push(b(p.b));
            out.push(b(p.g));
            out.push(b(p.r));
        }
        for _ in 0..(row_stride - img.width * 3) {
            out.push(0);
        }
    }
    out
}

pub fn decode_bmp(bytes: &[u8]) -> Result<BmpImage, GodotError> {
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err(GodotError::Parse("not a BMP file".into()));
    }
    let u16 = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32 = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let data_off = u32(10) as usize;
    let w = u32(18);
    let h = u32(22);
    if u16(28) != 24 || u32(30) != 0 {
        return Err(GodotError::Parse("only uncompressed 24-bit BMP in v1".into()));
    }
    let stride = ((w * 3 + 3) / 4) * 4;
    if bytes.len() < data_off + (stride * h) as usize {
        return Err(GodotError::Parse("BMP truncated".into()));
    }
    let mut pixels = vec![Color::BLACK; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let o = data_off + ((h - 1 - y) * stride + x * 3) as usize;
            pixels[(y * w + x) as usize] =
                Color::rgb(bytes[o + 2] as f32 / 255.0, bytes[o + 1] as f32 / 255.0, bytes[o] as f32 / 255.0);
        }
    }
    Ok(BmpImage { width: w, height: h, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_roundtrip_matches_encoder() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        // Reuse the servers encoder shape (header + i16le).
        let mut bytes = vec![];
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&44100u32.to_le_bytes());
        bytes.extend_from_slice(&88200u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
        for s in &samples {
            bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
        }
        let wav = decode_wav(&bytes).unwrap();
        assert_eq!(wav.rate, 44100);
        assert_eq!(wav.samples.len(), samples.len());
        assert!((wav.samples[1] - 0.5).abs() < 1e-4);
    }
    #[test]
    fn png_roundtrip_preserves_pixels() {
        use image::codecs::png::PngEncoder;
        use image::{ExtendedColorType, ImageBuffer, ImageEncoder, Rgba};
        let raw: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(2, 2, |x, y| Rgba([(x * 127) as u8, (y * 127) as u8, 64, 255]));
        let mut bytes = vec![];
        PngEncoder::new(&mut bytes)
            .write_image(&raw.clone().into_raw(), 2, 2, ExtendedColorType::Rgba8)
            .unwrap();
        let back = decode_image(&bytes).unwrap();
        assert_eq!((back.width, back.height), (2, 2));
        assert!((back.pixels[3].r - 127.0 / 255.0).abs() < 0.01);
        assert!((back.pixels[3].g - 127.0 / 255.0).abs() < 0.01);
        assert!(decode_image(b"definitely not an image").is_err());
    }
    #[test]
    fn ogg_rejects_garbage_gracefully() {
        assert!(decode_ogg(b"not ogg data at all............").is_err());
        assert!(decode_ogg(b"").is_err());
    }
    #[test]
    fn bmp_roundtrip_preserves_pixels() {
        let img = BmpImage {
            width: 2,
            height: 2,
            pixels: vec![Color::RED, Color::GREEN, Color::BLUE, Color::WHITE],
        };
        let bytes = encode_bmp(&img);
        assert_eq!(&bytes[0..2], b"BM");
        let back = decode_bmp(&bytes).unwrap();
        assert_eq!((back.width, back.height), (2, 2));
        assert!((back.pixels[0].r - 1.0).abs() < 0.01 && back.pixels[0].g < 0.01);
        assert!(decode_bmp(b"nope").is_err());
    }
}
