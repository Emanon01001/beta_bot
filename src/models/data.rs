use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;

use crate::util::{queue::MusicQueue, types::PlayingMap};

pub struct Data {
    /// ギルドごとの再生待ちキュー
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    /// ギルドごとの現在再生中ハンドル
    pub playing: PlayingMap,
}

impl Data {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(DashMap::new()),
            playing: Arc::new(DashMap::new()),
        }
    }
}