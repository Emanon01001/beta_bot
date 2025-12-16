use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dashmap::DashMap;
use poise::serenity_prelude::{ChannelId, GuildId, MessageId};
use songbird::tracks::TrackHandle;

use crate::util::track::TrackRequest;

pub type PlayingMap = Arc<DashMap<GuildId, (TrackHandle, TrackRequest)>>;
pub type TransitionFlags = Arc<DashMap<GuildId, Arc<AtomicBool>>>;
pub type HistoryMap = Arc<DashMap<GuildId, VecDeque<TrackRequest>>>;
pub type NowPlayingMap = Arc<DashMap<GuildId, (ChannelId, MessageId)>>;
