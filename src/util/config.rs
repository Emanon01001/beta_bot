use songbird::tracks::PlayMode;

use crate::util::repeat::RepeatMode;

#[derive(Debug)]
pub struct MusicConfig {
    pub repeat_mode: RepeatMode,
    pub play_mode: PlayMode,
    pub volume: f32,
}

impl MusicConfig {
    pub fn new() -> Self {
        Self {
            repeat_mode: RepeatMode::Off,
            play_mode: PlayMode::Stop,
            volume: 1.0,
        }
    }
}
