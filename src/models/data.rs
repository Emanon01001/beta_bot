use std::sync::Arc;

use dashmap::DashMap;
use lavalink_rs::client::LavalinkClient;
use poise::serenity_prelude::GuildId;

use crate::util::{
    queue::MusicQueue,
    types::{HistoryMap, LavalinkPlayingMap, NowPlayingMap, TransitionFlags},
};

pub struct Data {
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    pub lavalink_playing: LavalinkPlayingMap,
    pub transition_flags: TransitionFlags,
    pub history: HistoryMap,
    pub now_playing: NowPlayingMap,
    pub lavalink: Option<Arc<LavalinkClient>>,
}

impl Data {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(DashMap::new()),
            lavalink_playing: Arc::new(DashMap::new()),
            transition_flags: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            now_playing: Arc::new(DashMap::new()),
            lavalink: None,
        }
    }
}
