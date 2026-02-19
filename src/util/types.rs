use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dashmap::DashMap;
use poise::serenity_prelude::{ChannelId, GuildId, MessageId};

use crate::util::track::TrackRequest;

pub type LavalinkPlayingMap = Arc<DashMap<GuildId, TrackRequest>>;
pub type TransitionFlags = Arc<DashMap<GuildId, Arc<AtomicBool>>>;
pub type HistoryMap = Arc<DashMap<GuildId, VecDeque<TrackRequest>>>;
pub type NowPlayingMap = Arc<DashMap<GuildId, (ChannelId, MessageId)>>;
