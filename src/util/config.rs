use crate::util::repeat::RepeatMode;

#[derive(Debug)]
pub struct MusicConfig {
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
    pub volume: f32,
}

impl MusicConfig {
    pub fn new() -> Self {
        Self {
            repeat_mode: RepeatMode::Off,
            shuffle: false,
            // デフォルトの音量は 0.7 (70%)
            volume: 0.7,
        }
    }
}
