use crate::util::repeat::RepeatMode;

#[derive(Debug)]
pub struct MusicConfig {
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
}

impl MusicConfig {
    pub fn new() -> Self {
        Self {
            repeat_mode: RepeatMode::Off,
            shuffle: false,
        }
    }
}
