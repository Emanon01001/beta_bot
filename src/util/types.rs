use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::tracks::TrackHandle;

use crate::util::track::TrackRequest;

pub type PlayingMap = Arc<DashMap<GuildId, (TrackHandle, TrackRequest)>>;
