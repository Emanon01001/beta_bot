use crate::util::{config::MusicConfig, track::TrackRequest};
use std::collections::VecDeque;

#[derive(Debug)]
pub struct MusicQueue {
    queue: VecDeque<TrackRequest>,
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
    pub fn new() -> Self {
        Self::default()
    }

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
        self.queue.pop_front()
    }

    /// 参照イテレータ（cloneしない）
    pub fn iter(&self) -> impl Iterator<Item = &TrackRequest> {
        self.queue.iter()
    }

    /// コピー用途（UIなどで所有権必要）
    pub fn to_vec(&self) -> Vec<TrackRequest> {
        self.queue.iter().cloned().collect()
    }
    /// キューの長さ
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    // キューが空かどうか
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }

    //volume get
    pub fn volume(&self) -> f32 {
        self.config.volume
    }
    //volume set
    pub fn set_volume(&mut self, v: f32) {
        self.config.volume = v;
    }
}
