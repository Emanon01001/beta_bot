use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;

use crate::util::{
    queue::MusicQueue,
    types::{HistoryMap, NowPlayingMap, PlayingMap, TransitionFlags},
};

pub struct Data {
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    pub playing: PlayingMap,
    pub transition_flags: TransitionFlags,
    pub history: HistoryMap,
    pub now_playing: NowPlayingMap,
}

impl Data {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(DashMap::new()),
            playing: Arc::new(DashMap::new()),
            transition_flags: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            now_playing: Arc::new(DashMap::new()),
        }
    }
}
