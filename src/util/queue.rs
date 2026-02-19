use rand::RngExt;

use crate::util::{config::MusicConfig, repeat::RepeatMode, track::TrackRequest};
use std::collections::VecDeque;

#[derive(Debug)]
pub struct MusicQueue {
    pub queue: VecDeque<TrackRequest>,
    pub config: MusicConfig,
}

// Implement Default for MusicQueue
impl Default for MusicQueue {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            config: MusicConfig::new(),
        }
    }
}

// MusicQueueのメソッドを実装
impl MusicQueue {
    /// 末尾に追加（一般ユーザー用）
    pub fn push_back(&mut self, req: TrackRequest) {
        self.queue.push_back(req);
    }

    /// 先頭に追加（管理者／優先再生用）
    pub fn push_front(&mut self, req: TrackRequest) {
        self.queue.push_front(req);
    }

    /// 次に再生する曲（front）を取り出す
    pub fn pop_next(&mut self) -> Option<TrackRequest> {
        if self.config.shuffle {
            let len = self.queue.len();
            if len == 0 {
                return None;
            }
            let idx = rand::rng().random_range(0..len);
            self.queue.remove(idx)
        } else {
            self.queue.pop_front()
        }
    }

    /// 参照イテレータ（cloneしない）
    pub fn iter(&self) -> impl Iterator<Item = &TrackRequest> {
        self.queue.iter()
    }
    pub fn remove_at(&mut self, idx: usize) -> Option<TrackRequest> {
        if idx < self.queue.len() {
            self.queue.remove(idx)
        } else {
            None
        }
    }
    /// キューの長さ
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.config.repeat_mode = mode;
    }
    pub fn set_shuffle(&mut self, on: bool) {
        self.config.shuffle = on;
    }
}
