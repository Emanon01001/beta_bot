use dashmap::DashMap;
use poise::serenity_prelude::{async_trait, GuildId};
use songbird::{Call, EventContext, EventHandler, Event};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::util::queue::MusicQueue;

pub struct TrackEndHandler {
    pub guild_id: GuildId,
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    pub call: Arc<Mutex<Call>>,
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        // 1️⃣ next を取り出す──この時点で DashMap のシャードロックを保持
        let next_req_opt = {
            if let Some(mut q) = self.queues.get_mut(&self.guild_id) {
                q.pop_next()           // TrackRequest を取り出し
            } else { None }
        };                              // ここで q がドロップ→ロック解放

        // 2️⃣ 取り出せたら再生
        if let Some(next_req) = next_req_opt {
            // 新しい TrackEndHandler を組み立て
            let next_handler = TrackEndHandler {
                guild_id: self.guild_id,
                queues:   self.queues.clone(),
                call:     self.call.clone(),
            };

            if let Err(e) = crate::util::play::play_track(
                self.call.clone(),
                next_req,
                Some(next_handler),
            )
            .await
            {
                tracing::error!("TrackEndHandler: failed to play next: {:?}", e);
            }
        }
        None
    }
}