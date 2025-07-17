use dashmap::DashMap;
use poise::serenity_prelude::{async_trait, GuildId};
use songbird::{tracks::TrackHandle, Call, Event, EventContext, EventHandler};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::util::queue::MusicQueue;

#[derive(Clone)]
pub struct TrackEndHandler {
    pub guild_id: GuildId,
    pub queues  : Arc<DashMap<GuildId, MusicQueue>>,
    pub call    : Arc<Mutex<Call>>,
    pub playing : Arc<DashMap<GuildId, TrackHandle>>, // ← 追加しておくと掃除できる
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        // 次曲を取得（ロックは短く）
        let next_opt = {
            if let Some(mut q) = self.queues.get_mut(&self.guild_id) {
                q.pop_next()
            } else { None }
        };

        if let Some(next_req) = next_opt {
            let h = TrackEndHandler {
                guild_id: self.guild_id,
                queues:   self.queues.clone(),
                call:     self.call.clone(),
                playing:  self.playing.clone(),
            };
            if let Ok((handle, _)) = crate::util::play::play_track(self.call.clone(), next_req, Some(h.clone())).await {
                // 新しいハンドルを保存
                self.playing.insert(self.guild_id, handle);
            } else {
                tracing::error!("Failed to play next track");
                self.playing.remove(&self.guild_id); // 壊れたハンドルは掃除
            }
        } else {
            // キューが空ならハンドルも掃除
            self.playing.remove(&self.guild_id);
        }
        None
    }
}
