//! Full port of `servers/audio/audio_server.h/.cpp` + `audio_driver_dummy.h`.
//!
//! The dummy driver is upstream's headless strategy: a sample-accurate clock
//! with no hardware. Players mix analytically (`render_pcm`), so tests and
//! agents verify audio without speakers. Real output drivers plug in under
//! `godot_host` (module 04) behind the same API.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SAMPLE_RATE: u32 = 44100;

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub name: String,
    pub volume_db: f32,
    pub mute: bool,
    pub solo: bool,
    /// Peak of the last `render_pcm` (0..1), for meters + MCP inspect.
    pub peak: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stream {
    Sine { freq: f32 },
    Square { freq: f32 },
    Noise { seed: u64 },
    /// Mono PCM at its own rate (resampled by nearest neighbor in v1).
    Pcm { data: Vec<f32>, rate: u32 },
}

impl Stream {
    pub fn duration(&self) -> Option<f64> {
        match self {
            Stream::Sine { .. } | Stream::Square { .. } | Stream::Noise { .. } => None, // infinite
            Stream::Pcm { data, rate } => Some(data.len() as f64 / *rate as f64),
        }
    }
    fn sample(&self, t: f64, phase: f64) -> f32 {
        let _ = phase;
        match *self {
            Stream::Sine { freq } => (t * freq as f64 * std::f64::consts::TAU).sin() as f32,
            Stream::Square { freq } => {
                if (t * freq as f64).fract() < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Stream::Noise { seed } => {
                // Deterministic xorshift from (seed, sample index).
                let i = (t * SAMPLE_RATE as f64) as u64;
                let mut x = seed.wrapping_add(i.wrapping_mul(0x9E3779B97F4A7C15));
                x ^= x >> 30;
                x = x.wrapping_mul(0xBF58476D1CE4E5B9);
                x ^= x >> 27;
                x = x.wrapping_mul(0x94D049BB133111EB);
                x ^= x >> 31;
                ((x >> 11) as f32 / (1u64 << 53) as f32) * 2.0 - 1.0
            }
            Stream::Pcm { .. } => 0.0, // indexed path in render (needs rate conversion)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub stream: Stream,
    pub bus: usize,
    pub volume_db: f32,
    pub pitch: f32,
    pub playing: bool,
    pub paused: bool,
    pub position: f64,
    pub finished: bool,
}

pub struct AudioServer {
    buses: Vec<Bus>,
    players: BTreeMap<u64, Player>,
    next_player: u64,
    pub time: f64,
}

impl AudioServer {
    pub fn new() -> Self {
        Self {
            buses: vec![Bus { name: "Master".into(), volume_db: 0.0, mute: false, solo: false, peak: 0.0 }],
            players: BTreeMap::new(),
            next_player: 1,
            time: 0.0,
        }
    }
    pub fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    // ------------------------------------------------------------- buses ---
    pub fn bus_create(&mut self, name: impl Into<String>) -> usize {
        self.buses.push(Bus { name: name.into(), volume_db: 0.0, mute: false, solo: false, peak: 0.0 });
        self.buses.len() - 1
    }
    pub fn bus_count(&self) -> usize {
        self.buses.len()
    }
    pub fn bus_name(&self, bus: usize) -> Option<&str> {
        self.buses.get(bus).map(|b| b.name.as_str())
    }
    pub fn bus_set_volume(&mut self, bus: usize, db: f32) -> bool {
        if let Some(b) = self.buses.get_mut(bus) {
            b.volume_db = db.clamp(-80.0, 24.0);
            return true;
        }
        false
    }
    pub fn bus_set_mute(&mut self, bus: usize, mute: bool) -> bool {
        if let Some(b) = self.buses.get_mut(bus) {
            b.mute = mute;
            return true;
        }
        false
    }
    pub fn bus_set_solo(&mut self, bus: usize, solo: bool) -> bool {
        if let Some(b) = self.buses.get_mut(bus) {
            b.solo = solo;
            return true;
        }
        false
    }
    pub fn bus_peak(&self, bus: usize) -> Option<f32> {
        self.buses.get(bus).map(|b| b.peak)
    }
    fn bus_gain(&self, bus: usize) -> f32 {
        let any_solo = self.buses.iter().any(|b| b.solo);
        let Some(b) = self.buses.get(bus) else { return 0.0 };
        if b.mute {
            return 0.0;
        }
        if any_solo && !b.solo {
            return 0.0;
        }
        db_to_linear(b.volume_db)
    }

    // ----------------------------------------------------------- players ---
    pub fn player_create(&mut self, stream: Stream, bus: usize) -> u64 {
        let id = self.next_player;
        self.next_player += 1;
        self.players.insert(
            id,
            Player {
                stream,
                bus: bus.min(self.buses.len().saturating_sub(1)),
                volume_db: 0.0,
                pitch: 1.0,
                playing: false,
                paused: false,
                position: 0.0,
                finished: false,
            },
        );
        id
    }
    pub fn play(&mut self, player: u64, from: f64) -> bool {
        if let Some(p) = self.players.get_mut(&player) {
            p.playing = true;
            p.paused = false;
            p.finished = false;
            p.position = from.max(0.0);
            return true;
        }
        false
    }
    pub fn stop(&mut self, player: u64) -> bool {
        if let Some(p) = self.players.get_mut(&player) {
            p.playing = false;
            p.position = 0.0;
            return true;
        }
        false
    }
    pub fn pause(&mut self, player: u64, paused: bool) -> bool {
        if let Some(p) = self.players.get_mut(&player) {
            p.paused = paused;
            return true;
        }
        false
    }
    pub fn seek(&mut self, player: u64, to: f64) -> bool {
        if let Some(p) = self.players.get_mut(&player) {
            p.position = to.max(0.0);
            p.finished = false;
            return true;
        }
        false
    }
    pub fn set_volume(&mut self, player: u64, db: f32) -> bool {
        if let Some(p) = self.players.get_mut(&player) {
            p.volume_db = db.clamp(-80.0, 24.0);
            return true;
        }
        false
    }
    pub fn is_playing(&self, player: u64) -> bool {
        self.players.get(&player).map(|p| p.playing && !p.paused && !p.finished).unwrap_or(false)
    }
    pub fn playback_position(&self, player: u64) -> Option<f64> {
        self.players.get(&player).map(|p| p.position)
    }

    /// Advance the dummy clock without rendering (drives `finished` flags).
    pub fn tick(&mut self, dt: f64) {
        self.time += dt;
        for p in self.players.values_mut() {
            if !p.playing || p.paused || p.finished {
                continue;
            }
            p.position += dt * p.pitch.max(0.01) as f64;
            if let Some(dur) = p.stream.duration() {
                let rate_scale = match &p.stream {
                    Stream::Pcm { rate, .. } => *rate as f64 / SAMPLE_RATE as f64,
                    _ => 1.0,
                };
                if p.position >= dur * rate_scale {
                    p.finished = true;
                    p.playing = false;
                }
            }
        }
    }

    /// Mix `seconds` of audio at 44100 Hz mono. Advances positions + clock.
    pub fn render_pcm(&mut self, seconds: f64) -> Vec<f32> {
        let n = (seconds * SAMPLE_RATE as f64).round().max(1.0) as usize;
        let mut out = vec![0f32; n];
        let mut peaks = vec![0f32; self.buses.len()];
        for i in 0..n {
            let t = self.time + i as f64 / SAMPLE_RATE as f64;
            let mut frame = 0f32;
            for p in self.players.values() {
                if !p.playing || p.paused || p.finished {
                    continue;
                }
                let pt = p.position + i as f64 / SAMPLE_RATE as f64 * p.pitch.max(0.01) as f64;
                let s = match &p.stream {
                    Stream::Pcm { data, rate } => {
                        let idx = (pt * *rate as f64) as usize;
                        if idx < data.len() {
                            data[idx]
                        } else {
                            0.0
                        }
                    }
                    other => other.sample(t, pt),
                };
                let g = db_to_linear(p.volume_db) * self.bus_gain(p.bus);
                frame += s * g;
                peaks[p.bus] = peaks[p.bus].max((s * g).abs());
            }
            out[i] = frame.clamp(-1.0, 1.0);
        }
        // Commit positions + peaks + finished.
        let adv = seconds;
        for (bi, peak) in peaks.into_iter().enumerate() {
            if let Some(b) = self.buses.get_mut(bi) {
                b.peak = peak;
            }
        }
        // Collect finished PCM players.
        let done: Vec<u64> = self
            .players
            .iter()
            .filter_map(|(id, p)| match &p.stream {
                Stream::Pcm { data, rate } => {
                    let dur = data.len() as f64 / *rate as f64 / p.pitch.max(0.01) as f64;
                    (p.playing && p.position + adv >= dur).then_some(*id)
                }
                _ => None,
            })
            .collect();
        for id in done {
            if let Some(p) = self.players.get_mut(&id) {
                p.finished = true;
                p.playing = false;
            }
        }
        for p in self.players.values_mut() {
            if p.playing && !p.paused {
                p.position += adv * p.pitch.max(0.01) as f64;
            }
        }
        self.time += seconds;
        out
    }

    /// 16-bit PCM WAV bytes (mono, 44100 Hz) — `export` path for MCP/agents.
    pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(44 + samples.len() * 2);
        let write_u32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
        let write_u16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(b"RIFF");
        write_u32(&mut out, 36 + samples.len() as u32 * 2);
        out.extend_from_slice(b"WAVEfmt ");
        write_u32(&mut out, 16);
        write_u16(&mut out, 1); // PCM
        write_u16(&mut out, 1); // mono
        write_u32(&mut out, SAMPLE_RATE);
        write_u32(&mut out, SAMPLE_RATE * 2);
        write_u16(&mut out, 2);
        write_u16(&mut out, 16);
        out.extend_from_slice(b"data");
        write_u32(&mut out, samples.len() as u32 * 2);
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }
}

impl Default for AudioServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn zero_crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
    }
    #[test]
    fn sine_mixes_at_right_pitch() {
        let mut a = AudioServer::new();
        let p = a.player_create(Stream::Sine { freq: 440.0 }, 0);
        a.play(p, 0.0);
        let pcm = a.render_pcm(0.1);
        assert_eq!(pcm.len(), 4410);
        // 440 Hz × 0.1 s = 44 cycles = ~88 zero crossings (±2 for phase).
        let zc = zero_crossings(&pcm);
        assert!((86..=90).contains(&zc), "got {zc} crossings");
        assert!(a.is_playing(p));
    }
    #[test]
    fn mute_and_volume_gate_output() {
        let mut a = AudioServer::new();
        let p = a.player_create(Stream::Sine { freq: 440.0 }, 0);
        a.play(p, 0.0);
        a.bus_set_mute(0, true);
        let pcm = a.render_pcm(0.05);
        assert!(pcm.iter().all(|s| *s == 0.0));
        a.bus_set_mute(0, false);
        a.set_volume(p, -80.0);
        let pcm = a.render_pcm(0.05);
        assert!(pcm.iter().map(|s| s.abs()).fold(0f32, f32::max) < 0.001);
    }
    #[test]
    fn pcm_stream_finishes_and_wav_header_valid() {
        let mut a = AudioServer::new();
        let data = vec![0.5f32; SAMPLE_RATE as usize]; // 1 second
        let p = a.player_create(Stream::Pcm { data, rate: SAMPLE_RATE }, 0);
        a.play(p, 0.0);
        let pcm = a.render_pcm(1.1);
        assert!(!a.is_playing(p));
        let wav = AudioServer::encode_wav(&pcm);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + pcm.len() * 2);
    }
}
