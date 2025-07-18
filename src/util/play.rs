// src/util/play.rs
use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::{
    input::{Compose, LiveInput, YoutubeDl},
    tracks::{Track, TrackHandle},
    Call, Event, TrackEvent,
};
use tokio::sync::Mutex;
use url::Url;

use crate::{
    Error, get_http_client,
    handlers::track_end::TrackEndHandler,
    util::{queue::MusicQueue, track::TrackRequest, types::PlayingMap},
};

pub async fn play_track_req(
    guild_id: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: PlayingMap,
    track_req: TrackRequest,
) -> Result<(TrackHandle, TrackRequest), Error> {
    let handler = TrackEndHandler {
        guild_id,
        queues,
        call: call.clone(),
        playing: playing.clone(),
    };

    let (handle, req) = play_track(call, track_req, Some(handler)).await?;
    playing.insert(guild_id, (handle.clone(), req.clone()));
    Ok((handle, req))
}

pub async fn play_track(
    call: Arc<Mutex<Call>>,
    mut track_req: TrackRequest,
    on_end: Option<TrackEndHandler>,
) -> Result<(TrackHandle, TrackRequest), Error> {
    // URL 判定
    let url = track_req.url.clone();
    let mut ytdl = if Url::parse(&url).is_ok() {
        YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), url)
    } else {
        YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), url)
    };

    let audio = ytdl.create_async().await.map_err(Error::from)?;
    track_req.meta = ytdl.aux_metadata().await.map_err(Error::from)?;
    let input = songbird::input::Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl)));

    let handle = {
        let mut g = call.lock().await;
        g.play_only(Track::from(input))
    };

    if let Some(h) = on_end {
        handle.add_event(Event::Track(TrackEvent::End), h.clone()).ok();
        handle.add_event(Event::Track(TrackEvent::Error), h).ok();
    }

    Ok((handle, track_req))
}