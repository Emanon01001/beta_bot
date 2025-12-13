// Do not force GUI subsystem here; main handles console behavior.

use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::{
    Call, Event, TrackEvent,
    input::{Compose, Input, LiveInput, YoutubeDl},
    tracks::{Track, TrackHandle},
};
use tokio::{
    sync::Mutex,
    time::{Duration, timeout},
};

use crate::{
    Error, get_http_client,
    handlers::track_end::TrackEndHandler,
    util::{
        queue::MusicQueue,
        track::TrackRequest,
        types::PlayingMap,
        ytdlp::{cookies_args, extra_args_from_config},
    },
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
    let handler = TrackEndHandler {
        guild_id: gid,
        queues,
        call: call.clone(),
        playing: playing.clone(),
    };

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

    // URL/検索語を実オーディオストリームに解決し、必要ならメタデータでURLを安定化。
    let input = resolve_input(&mut tr).await?;
    let title = tr.meta.title.as_deref().unwrap_or(&url);
    tracing::info!(%title, "Starting playback");

    let handle = { call.lock().await.play_only(Track::from(input)) };
    if let Some(ev) = on_end {
        handle
            .add_event(Event::Track(TrackEvent::End), ev.clone())
            .ok();
        handle.add_event(Event::Track(TrackEvent::Error), ev).ok();
    }
    Ok((handle, tr))
}

/// yt-dlp を使って入力をストリームに変換し、メタデータを可能な範囲で保持する。
async fn resolve_input(tr: &mut TrackRequest) -> Result<Input, Error> {
    if is_soundcloud(&tr.url) {
        tracing::info!("Source: SoundCloud URL");
        let mut source = YoutubeDl::new_search(get_http_client(), tr.url.to_string())
            .user_args(vec![
                "-4".into(),
                "--ignore-config".into(),
                "--no-warnings".into(),
                "--no-playlist".into(),
                "-f".into(),
                "http_mp3_0_1/http_mp3_0_0/bestaudio[acodec=mp3][protocol^=http]".into(),
            ])
            .user_args(cookies_args())
            .user_args(extra_args_from_config());
        let fut = source.create_async();
        let audio = match timeout(Duration::from_secs(20), fut).await {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => return Err(Error::from(format!("yt-dlp (SoundCloud) 実行失敗: {e}"))),
            Err(_) => return Err(Error::from("yt-dlp (SoundCloud) がタイムアウトしました")),
        };
        return Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(source))));
    }
    if is_youtube(&tr.url) {
        tracing::info!("Source: YouTube URL");
        let mut ytdl = YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), tr.url.to_string())
            .user_args(vec![
                "-4".into(),
                "--ignore-config".into(),
                "--no-warnings".into(),
                "--no-playlist".into(),
                "--geo-bypass".into(),
                "-f".into(),
                "bestaudio[protocol^=http]/bestaudio".into(),
            ])
            .user_args(cookies_args())
            .user_args(extra_args_from_config());
        if let Ok(meta) = ytdl.aux_metadata().await {
            tr.meta = meta;
            if let Some(src) = tr.meta.source_url.clone() {
                tr.url = src;
            }
        }
        let fut = ytdl.create_async();
        let audio = match timeout(Duration::from_secs(20), fut).await {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => return Err(Error::from(format!("yt-dlp (YouTube) 実行失敗: {e}"))),
            Err(_) => return Err(Error::from("yt-dlp (YouTube) がタイムアウトしました")),
        };
        return Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl))));
    }

    tracing::info!("Source: yt-dlp search (ytsearch1:, opus-priority)");
    let mut ytdl = YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), tr.url.to_string())
        .user_args(vec![
            "-4".into(),
            "--ignore-config".into(),
            "--no-warnings".into(),
            "--no-playlist".into(),
            "--geo-bypass".into(),
            "-f".into(),
            "bestaudio[protocol^=http]/bestaudio".into(),
        ])
        .user_args(cookies_args())
        .user_args(extra_args_from_config());
    // 検索: まず通常パラメータで試行
    let fut = ytdl.create_async();
    let audio = match timeout(Duration::from_secs(20), fut).await {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            // フォールバック: できるだけ素の条件で再試行（フォーマット/ジオ/警告抑止を外す）
            tracing::warn!(error=%e, "yt-dlp 検索の初回実行に失敗。簡易引数で再試行します");
            let mut ytdl2 =
                YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), tr.url.to_string());
            match timeout(Duration::from_secs(20), ytdl2.create_async()).await {
                Ok(Ok(a2)) => a2,
                Ok(Err(e2)) => return Err(Error::from(format!("yt-dlp (検索) 実行失敗: {e2}"))),
                Err(_) => return Err(Error::from("yt-dlp (検索) がタイムアウトしました")),
            }
        }
        Err(_) => return Err(Error::from("yt-dlp (検索) がタイムアウトしました")),
    };
    if let Ok(meta) = ytdl.aux_metadata().await {
        tr.meta = meta;
        if let Some(src) = tr.meta.source_url.clone() {
            tr.url = src;
        }
    }

    Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl))))
}
