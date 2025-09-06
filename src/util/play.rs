use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::{
    input::{Compose, Input, LiveInput, YoutubeDl}, tracks::{Track, TrackHandle}, Call, Event, TrackEvent
};
use tokio::sync::Mutex;

use crate::{
    handlers::track_end::TrackEndHandler, Error, get_http_client,
    util::{queue::MusicQueue, track::TrackRequest, types::PlayingMap},
};

pub fn is_youtube(u: &str) -> bool {
    u.contains("youtube.com") || u.contains("youtu.be") || u.contains("m.youtube.com")
}

pub fn is_soundcloud(u: &str) -> bool {
    u.contains("soundcloud.com") || u.contains("snd.sc") || u.contains("sndcdn.com")
}

pub async fn play_track_req(
    gid: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: PlayingMap,
    tr: TrackRequest,
) -> Result<(TrackHandle, TrackRequest), Error> {
    tracing::info!(guild = %gid, url = %tr.url, "Play request");
    let handler = TrackEndHandler { guild_id: gid, queues, call: call.clone(), playing: playing.clone() };

    let (h, req) = play_track(call, tr, Some(handler)).await?;
    playing.insert(gid, (h.clone(), req.clone()));
    Ok((h, req))
}

pub async fn play_track(
    call: Arc<Mutex<Call>>,
    mut tr: TrackRequest,
    on_end: Option<TrackEndHandler>,
) -> Result<(TrackHandle, TrackRequest), Error> {
    let url = tr.url.clone();
    tracing::info!(%url, "Processing track");

    let input = resolve_input(&mut tr).await?;
    let title = tr.meta.title.as_deref().unwrap_or(&url);
    tracing::info!(%title, "Starting playback");

    let handle = { call.lock().await.play_only(Track::from(input)) };
    if let Some(ev) = on_end {
        handle.add_event(Event::Track(TrackEvent::End), ev.clone()).ok();
        handle.add_event(Event::Track(TrackEvent::Error), ev).ok();
    }
    Ok((handle, tr))
}

async fn resolve_input(tr: &mut TrackRequest) -> Result<Input, Error> {
    if is_soundcloud(&tr.url) {
        tracing::info!("Source: SoundCloud URL");
        let mut source = YoutubeDl::new_search(get_http_client(), tr.url.to_string())
            .user_args(vec!["-f".into(), "http_mp3_0_1/http_mp3_0_0/bestaudio[acodec=mp3][protocol^=http]".into()]);
        let audio = source.create_async().await.map_err(Error::from)?;
        return Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(source))));
    }
    if is_youtube(&tr.url) {
        tracing::info!("Source: YouTube URL");
        let mut ytdl = YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), tr.url.to_string());
        if let Ok(meta) = ytdl.aux_metadata().await { tr.meta = meta; }
        return Ok(Input::Live(LiveInput::Raw(ytdl.create_async().await.map_err(Error::from)?), Some(Box::new(ytdl))));
    }

    tracing::info!("Source: yt-dlp search (ytsearch1:, opus-priority)");
    let mut ytdl = YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), tr.url.to_string());
    let audio = ytdl.create_async().await.map_err(Error::from)?;
    if let Ok(meta) = ytdl.aux_metadata().await { tr.meta = meta; }

    Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl))))
}