use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use poise::serenity_prelude::GuildId;

use crate::util::{track::TrackRequest, types::TransitionFlags};

#[derive(Clone, Debug)]
pub enum PlaybackControlResult {
    Changed(TrackRequest),
    Unchanged,
    Missing,
}

pub struct ManualTransitionGuard {
    flag: Arc<AtomicBool>,
}

impl ManualTransitionGuard {
    pub fn acquire(flags: &TransitionFlags, guild_id: GuildId) -> Self {
        let flag = flags
            .entry(guild_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        flag.store(true, Ordering::Release);
        Self { flag }
    }
}

impl Drop for ManualTransitionGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
