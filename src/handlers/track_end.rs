use poise::serenity_prelude::async_trait;
use songbird::{Call, Event, EventContext, EventHandler};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::util::{play::play_track_req, queue::MusicQueue}; // ctx不要版想定

pub struct TrackEndHandler {
    pub queue: Arc<Mutex<MusicQueue>>,
    pub call: Arc<Mutex<Call>>,
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let next_req = {
            let mut q = self.queue.lock().await;
            q.pop_next()
        };

        if let Some(req) = next_req {
            if let Err(err) = play_track_req(self.call.clone(), self.queue.clone(), req).await {
                tracing::error!("Failed to auto-play next track: {err:?}");
            }
        }
        None
    }
}
