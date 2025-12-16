use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::collections::VecDeque;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::tracks::TrackHandle;

use crate::util::track::TrackRequest;

pub type PlayingMap = Arc<DashMap<GuildId, (TrackHandle, TrackRequest)>>;
pub type TransitionFlags = Arc<DashMap<GuildId, Arc<AtomicBool>>>;
pub type HistoryMap = Arc<DashMap<GuildId, VecDeque<TrackRequest>>>;
