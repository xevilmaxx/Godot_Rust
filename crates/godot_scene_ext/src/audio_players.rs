//! Port of `scene/audio/audio_stream_player.h` (node-data side).
//! Links a scene node to an `AudioServer` player id; the mix stays server-side.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStreamPlayer {
    pub name: String,
    pub stream_desc: String,
    pub bus: String,
    pub volume_db: f32,
    pub pitch: f32,
    pub autoplay: bool,
    pub playing_id: Option<u64>,
}

impl AudioStreamPlayer {
    pub fn new(name: &str, stream_desc: &str) -> Self {
        Self {
            name: name.into(),
            stream_desc: stream_desc.into(),
            bus: "Master".into(),
            volume_db: 0.0,
            pitch: 1.0,
            autoplay: false,
            playing_id: None,
        }
    }
    /// Bind to a live server player (created by the caller from `stream_desc`).
    pub fn bind(&mut self, id: u64) {
        self.playing_id = Some(id);
    }
    pub fn is_bound(&self) -> bool {
        self.playing_id.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_tracks_playback() {
        let mut p = AudioStreamPlayer::new("Music", "sine:440");
        assert!(!p.is_bound());
        p.bind(7);
        assert!(p.is_bound());
    }
}
