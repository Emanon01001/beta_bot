// src/handlers/track_end.rs
use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::{GuildId, async_trait};
use songbird::{Call, Event, EventContext, EventHandler};

use tokio::sync::Mutex;

use crate::util::{queue::MusicQueue, repeat::RepeatMode, track::TrackRequest, types::PlayingMap};

#[derive(Clone)]
pub struct TrackEndHandler {
    pub guild_id: GuildId,
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    pub call: Arc<Mutex<Call>>,
    pub playing: PlayingMap,
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let finished: Option<TrackRequest> =
            self.playing.remove(&self.guild_id).map(|(_, (_, r))| r);

        let next = {
            if let Some(mut q) = self.queues.get_mut(&self.guild_id) {
                if let Some(r) = finished.clone() {
                    match q.config.repeat_mode {
                        RepeatMode::Track => q.push_front(r),
                        RepeatMode::Queue => q.push_back(r),
                        RepeatMode::Off => {}
                    }
                }
                q.pop_next()
            } else {
                None
            }
        };

        if let Some(req) = next {
            let handler = self.clone();
            if let Ok((handle, r)) =
                crate::util::play::play_track(self.call.clone(), req, Some(handler)).await
            {
                self.playing.insert(self.guild_id, (handle, r));
            }
        }
        None
    }
}
