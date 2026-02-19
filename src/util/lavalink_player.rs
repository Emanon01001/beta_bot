use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use dashmap::DashMap;
use lavalink_rs::{
    client::LavalinkClient,
    model::{
        BoxFuture,
        client::NodeDistributionStrategy,
        events::{Events, TrackEnd, TrackStart},
        http::UpdatePlayer,
        search::SearchEngines,
        track::{Track as LavalinkTrack, TrackData, TrackLoadData},
    },
    node::NodeBuilder,
};
use poise::serenity_prelude::{Colour, EditMessage, GuildId, Http, UserId};
use songbird::{ConnectionInfo as SongbirdConnectionInfo, tracks::PlayMode};
use url::Url;

use crate::util::{
    alias::Context,
    music_ui::{control_components, track_embed},
    player::PlaybackControlResult,
    queue::MusicQueue,
    repeat::RepeatMode,
    track::TrackRequest,
    types::{HistoryMap, LavalinkPlayingMap, NowPlayingMap, TransitionFlags},
};
use crate::{Error, LavalinkSettings};

#[derive(Clone)]
pub struct LavalinkRuntimeData {
    pub queues: Arc<DashMap<GuildId, MusicQueue>>,
    pub transition_flags: TransitionFlags,
    pub history: HistoryMap,
    pub now_playing: NowPlayingMap,
    pub lavalink_playing: LavalinkPlayingMap,
    pub http: Arc<Http>,
}

pub struct PlayNextResult {
    pub started: Option<TrackRequest>,
    pub skipped: usize,
    pub remaining: usize,
    pub last_error: Option<String>,
}

const HISTORY_MAX: usize = 50;

fn first_track_from_load(load: LavalinkTrack) -> Result<Option<TrackData>, Error> {
    match load.data {
        Some(TrackLoadData::Track(track)) => Ok(Some(track)),
        Some(TrackLoadData::Search(tracks)) => Ok(tracks.into_iter().next()),
        Some(TrackLoadData::Playlist(playlist)) => Ok(playlist.tracks.into_iter().next()),
        Some(TrackLoadData::Error(err)) => Err(Error::from(format!(
            "Lavalink track load failed: {}",
            err.message
        ))),
        None => Ok(None),
    }
}

async fn resolve_track(
    lavalink: &LavalinkClient,
    guild_id: GuildId,
    identifier: &str,
) -> Result<TrackData, Error> {
    let load = lavalink
        .load_tracks(guild_id, identifier)
        .await
        .map_err(|e| Error::from(format!("Lavalink load_tracks error: {e}")))?;

    if let Some(track) = first_track_from_load(load)? {
        return Ok(track);
    }

    if Url::parse(identifier).is_err() {
        let query = SearchEngines::YouTube
            .to_query(identifier)
            .map_err(|e| Error::from(format!("Lavalink search query error: {e}")))?;
        let load = lavalink
            .load_tracks(guild_id, &query)
            .await
            .map_err(|e| Error::from(format!("Lavalink search load_tracks error: {e}")))?;
        if let Some(track) = first_track_from_load(load)? {
            return Ok(track);
        }
    }

    Err(Error::from("Lavalink did not return a playable track"))
}

fn apply_track_metadata(req: &mut TrackRequest, track: &TrackData) {
    req.meta.title = Some(track.info.title.clone());
    if !track.info.author.trim().is_empty() {
        req.meta.artist = Some(track.info.author.clone());
    }
    req.meta.duration = if track.info.is_stream {
        None
    } else {
        Some(Duration::from_millis(track.info.length))
    };
    req.meta.source_url = track.info.uri.clone().or_else(|| Some(req.url.clone()));
    req.meta.thumbnail = track.info.artwork_url.clone();
    if let Some(src) = req.meta.source_url.clone() {
        req.url = src;
    }
}

fn lavalink_track_start(
    _client: LavalinkClient,
    _session_id: String,
    event: &TrackStart,
) -> BoxFuture<'static, ()> {
    let event = event.clone();
    Box::pin(async move {
        tracing::info!(
            guild = event.guild_id.0,
            title = %event.track.info.title,
            "Lavalink track started"
        );
    })
}

fn lavalink_track_end(
    client: LavalinkClient,
    _session_id: String,
    event: &TrackEnd,
) -> BoxFuture<'static, ()> {
    let event = event.clone();
    Box::pin(async move {
        handle_track_end(client, event).await;
    })
}

async fn handle_track_end(client: LavalinkClient, event: TrackEnd) {
    let runtime = match client.data::<LavalinkRuntimeData>() {
        Ok(data) => data,
        Err(err) => {
            tracing::warn!(error = %err, "failed to fetch lavalink runtime data");
            return;
        }
    };

    let guild_id = GuildId::new(event.guild_id.0);
    if runtime
        .transition_flags
        .get(&guild_id)
        .is_some_and(|f| f.value().load(Ordering::Acquire))
    {
        return;
    }

    let finished = runtime
        .lavalink_playing
        .remove(&guild_id)
        .map(|(_, req)| req);

    if let Some(mut q) = runtime.queues.get_mut(&guild_id) {
        if let Some(req) = finished.clone() {
            match q.config.repeat_mode {
                RepeatMode::Track => q.push_front(req),
                RepeatMode::Queue => q.push_back(req),
                RepeatMode::Off => {}
            }
        }
    }

    let result = play_next_from_queue_lavalink(
        guild_id,
        Arc::new(client.clone()),
        runtime.queues.clone(),
        runtime.lavalink_playing.clone(),
        runtime.history.clone(),
        3,
    )
    .await;

    let Ok(result) = result else {
        tracing::warn!(guild = %guild_id, "failed to start next Lavalink track");
        return;
    };

    if let Some((channel_id, message_id)) = runtime.now_playing.get(&guild_id).map(|e| *e.value()) {
        if let Some(started) = result.started {
            let info = if result.skipped > 0 {
                format!(
                    "再生失敗 {} 件をスキップ / キュー残り {} 件",
                    result.skipped, result.remaining
                )
            } else {
                format!("キュー残り {} 件", result.remaining)
            };
            let embed = track_embed(
                "🎵 再生中",
                Some(&started),
                Some(info),
                Colour::new(0x2ECC71),
            );
            let _ = channel_id
                .edit_message(
                    &runtime.http,
                    message_id,
                    EditMessage::new()
                        .embeds(vec![embed])
                        .components(control_components(PlayMode::Play)),
                )
                .await;
        } else {
            let detail = result
                .last_error
                .unwrap_or_else(|| "キュー内に次の曲がありません".to_string());
            let embed = track_embed(
                "🎶 キュー再生が終了しました",
                None,
                Some(detail),
                Colour::new(0x5865F2),
            );
            let _ = channel_id
                .edit_message(
                    &runtime.http,
                    message_id,
                    EditMessage::new()
                        .embeds(vec![embed])
                        .components(Vec::new()),
                )
                .await;
        }
    }
}

fn build_events() -> Events {
    Events {
        track_start: Some(lavalink_track_start),
        track_end: Some(lavalink_track_end),
        ..Default::default()
    }
}

pub async fn build_lavalink_client(
    settings: &LavalinkSettings,
    user_id: UserId,
    runtime_data: LavalinkRuntimeData,
) -> Result<Arc<LavalinkClient>, Error> {
    let base_url = settings
        .base_url
        .as_deref()
        .map(str::trim)
        .ok_or_else(|| Error::from("lavalink base_url is missing"))?;
    if base_url.is_empty() {
        return Err(Error::from("lavalink base_url is empty"));
    }

    let parsed =
        Url::parse(base_url).map_err(|e| Error::from(format!("invalid lavalink base_url: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| Error::from("lavalink base_url host is missing"))?;
    let port = parsed.port_or_known_default().unwrap_or(2333);
    let hostname = format!("{host}:{port}");
    let is_ssl = parsed.scheme().eq_ignore_ascii_case("https");
    let password = settings
        .password
        .clone()
        .unwrap_or_else(|| "youshallnotpass".to_string());

    tracing::info!(
        host = %hostname,
        ssl = is_ssl,
        "initializing Lavalink client"
    );

    let node = NodeBuilder {
        hostname,
        is_ssl,
        password,
        user_id: user_id.into(),
        ..Default::default()
    };

    let client = LavalinkClient::new_with_data(
        build_events(),
        vec![node],
        NodeDistributionStrategy::sharded(),
        Arc::new(runtime_data),
    )
    .await;

    Ok(Arc::new(client))
}

pub async fn ensure_player_for_connection(
    lavalink: &LavalinkClient,
    guild_id: GuildId,
    connection: SongbirdConnectionInfo,
) -> Result<(), Error> {
    let connection = connection.into();
    if lavalink.get_player_context(guild_id).is_some() {
        lavalink
            .update_player(
                guild_id,
                &UpdatePlayer {
                    voice: Some(connection),
                    ..Default::default()
                },
                true,
            )
            .await
            .map_err(|e| {
                Error::from(format!("failed to update Lavalink player voice state: {e}"))
            })?;
    } else {
        lavalink
            .create_player_context(guild_id, connection)
            .await
            .map_err(|e| Error::from(format!("failed to create Lavalink player: {e}")))?;
    }

    Ok(())
}

pub async fn delete_player(lavalink: &LavalinkClient, guild_id: GuildId) -> Result<(), Error> {
    if lavalink.get_player_context(guild_id).is_some() {
        lavalink
            .delete_player(guild_id)
            .await
            .map_err(|e| Error::from(format!("failed to delete Lavalink player: {e}")))?;
    }
    Ok(())
}

pub async fn current_play_mode(lavalink: &LavalinkClient, guild_id: GuildId) -> PlayMode {
    let Some(player) = lavalink.get_player_context(guild_id) else {
        return PlayMode::Stop;
    };
    match player.get_player().await {
        Ok(state) if state.track.is_none() => PlayMode::Stop,
        Ok(state) if state.paused => PlayMode::Pause,
        Ok(_) => PlayMode::Play,
        Err(_) => PlayMode::Stop,
    }
}

pub async fn pause_current_lavalink(
    lavalink: &LavalinkClient,
    guild_id: GuildId,
    playing: &LavalinkPlayingMap,
) -> Result<PlaybackControlResult, Error> {
    let Some(req) = playing.get(&guild_id).map(|e| e.value().clone()) else {
        return Ok(PlaybackControlResult::Missing);
    };
    let Some(player) = lavalink.get_player_context(guild_id) else {
        return Ok(PlaybackControlResult::Missing);
    };
    let state = player
        .get_player()
        .await
        .map_err(|e| Error::from(format!("failed to get Lavalink player state: {e}")))?;
    if state.track.is_none() {
        return Ok(PlaybackControlResult::Missing);
    }
    if state.paused {
        return Ok(PlaybackControlResult::Unchanged);
    }
    player
        .set_pause(true)
        .await
        .map_err(|e| Error::from(format!("failed to pause Lavalink player: {e}")))?;
    Ok(PlaybackControlResult::Changed(req))
}

pub async fn resume_current_lavalink(
    lavalink: &LavalinkClient,
    guild_id: GuildId,
    playing: &LavalinkPlayingMap,
) -> Result<PlaybackControlResult, Error> {
    let Some(req) = playing.get(&guild_id).map(|e| e.value().clone()) else {
        return Ok(PlaybackControlResult::Missing);
    };
    let Some(player) = lavalink.get_player_context(guild_id) else {
        return Ok(PlaybackControlResult::Missing);
    };
    let state = player
        .get_player()
        .await
        .map_err(|e| Error::from(format!("failed to get Lavalink player state: {e}")))?;
    if state.track.is_none() {
        return Ok(PlaybackControlResult::Missing);
    }
    if !state.paused {
        return Ok(PlaybackControlResult::Unchanged);
    }
    player
        .set_pause(false)
        .await
        .map_err(|e| Error::from(format!("failed to resume Lavalink player: {e}")))?;
    Ok(PlaybackControlResult::Changed(req))
}

pub async fn stop_and_clear_lavalink(ctx: &Context<'_>, guild_id: GuildId) -> Result<(), Error> {
    if let Some(lavalink) = &ctx.data().lavalink {
        if let Some(player) = lavalink.get_player_context(guild_id) {
            let _ = player.stop_now().await;
        }
    }

    ctx.data().queues.remove(&guild_id);
    ctx.data().lavalink_playing.remove(&guild_id);
    ctx.data().history.remove(&guild_id);
    ctx.data().now_playing.remove(&guild_id);

    Ok(())
}

pub async fn play_track_req_lavalink(
    guild_id: GuildId,
    lavalink: Arc<LavalinkClient>,
    playing: LavalinkPlayingMap,
    history: HistoryMap,
    mut tr: TrackRequest,
) -> Result<TrackRequest, Error> {
    let track_data = resolve_track(&lavalink, guild_id, &tr.url).await?;
    apply_track_metadata(&mut tr, &track_data);

    let player = lavalink
        .get_player_context(guild_id)
        .ok_or_else(|| Error::from("Lavalink player is not connected to this guild"))?;
    player
        .play_now(&track_data)
        .await
        .map_err(|e| Error::from(format!("failed to start Lavalink playback: {e}")))?;

    playing.insert(guild_id, tr.clone());
    {
        let mut h = history.entry(guild_id).or_default();
        h.push_back(tr.clone());
        while h.len() > HISTORY_MAX {
            h.pop_front();
        }
    }

    Ok(tr)
}

pub async fn play_next_from_queue_lavalink(
    guild_id: GuildId,
    lavalink: Arc<LavalinkClient>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: LavalinkPlayingMap,
    history: HistoryMap,
    max_attempts: usize,
) -> Result<PlayNextResult, Error> {
    let mut skipped = 0usize;
    let mut last_error: Option<String> = None;
    let mut remaining = 0usize;

    for _ in 0..max_attempts.max(1) {
        let next_req = if let Some(mut q) = queues.get_mut(&guild_id) {
            let next = q.pop_next();
            remaining = q.len();
            next
        } else {
            remaining = 0;
            None
        };

        let Some(req) = next_req else {
            return Ok(PlayNextResult {
                started: None,
                skipped,
                remaining,
                last_error,
            });
        };

        match play_track_req_lavalink(
            guild_id,
            lavalink.clone(),
            playing.clone(),
            history.clone(),
            req,
        )
        .await
        {
            Ok(started_req) => {
                return Ok(PlayNextResult {
                    started: Some(started_req),
                    skipped,
                    remaining,
                    last_error,
                });
            }
            Err(err) => {
                last_error = Some(err.to_string());
                skipped += 1;
            }
        }
    }

    Ok(PlayNextResult {
        started: None,
        skipped,
        remaining,
        last_error,
    })
}
