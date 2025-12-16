// src/handlers/track_end.rs
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use poise::serenity_prelude::{GuildId, async_trait};
use songbird::{Call, Event, EventContext, EventHandler};

use tokio::sync::Mutex;

use crate::util::{
    queue::MusicQueue,
    repeat::RepeatMode,
    track::TrackRequest,
    types::{HistoryMap, PlayingMap, TransitionFlags},
};

#[derive(Clone)]
pub struct TrackEndHandler {
    pub guild_id: GuildId,
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    pub call: Arc<Mutex<Call>>,
    pub playing: PlayingMap,
    pub transition_flags: TransitionFlags,
    pub history: HistoryMap,
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        if self
            .transition_flags
            .get(&self.guild_id)
            .is_some_and(|f| f.value().load(Ordering::Acquire))
        {
            return None;
        }

        // Staleイベント(手動skip直後など)が現在のplayingを壊さないようにUUID一致を確認する。
        let event_uuid = match _ctx {
            EventContext::Track(tracks) => tracks.first().map(|(_, h)| h.uuid()),
            _ => None,
        };
        let current_uuid = self.playing.get(&self.guild_id).map(|e| e.value().0.uuid());
        if event_uuid.is_none() || current_uuid.is_none() || event_uuid != current_uuid {
            return None;
        }

        let finished: Option<TrackRequest> = self
            .playing
            .remove(&self.guild_id)
            .map(|(_, (_, r))| r);

        // 次の曲の開始。失敗したら数件スキップして続行する。
        let mut first = true;
        let mut tries = 0usize;
        while tries < 3 {
            let next = if let Some(mut q) = self.queues.get_mut(&self.guild_id) {
                if first {
                    // リピート設定に応じて再キュー。Trackは先頭、Queueは末尾。
                    if let Some(r) = finished.clone() {
                        match q.config.repeat_mode {
                            RepeatMode::Track => q.push_front(r),
                            RepeatMode::Queue => q.push_back(r),
                            RepeatMode::Off => {}
                        }
                    }
                    first = false;
                }
                q.pop_next()
            } else {
                None
            };

            let Some(req) = next else { break };

            match crate::util::play::play_track_req(
                self.guild_id,
                self.call.clone(),
                self.queues.clone(),
                self.playing.clone(),
                self.transition_flags.clone(),
                self.history.clone(),
                req,
            )
            .await
            {
                Ok((_handle, _r)) => break,
                Err(e) => {
                    tracing::warn!(guild = %self.guild_id, error = %e, "次曲の再生に失敗。次を試行します");
                    tries += 1;
                    continue;
                }
            };
        }
        None
    }
}
