//! src/util/play.rs  ― compact
use std::{process::Stdio, sync::Arc, time::Duration};

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use songbird::{
    input::{Compose, HlsRequest, HttpRequest, Input, LiveInput, YoutubeDl},
    tracks::{Track, TrackHandle},
    Call, Event, TrackEvent,
};
use tokio::{process::Command, sync::Mutex};
use url::Url;

use crate::{
    util::{queue::MusicQueue, track::TrackRequest, types::PlayingMap},
    Error, get_http_client,
    handlers::track_end::TrackEndHandler,
};

/* ─────────────── ヘルパ ─────────────── */
#[inline] fn is_youtube(u: &str) -> bool {
    u.contains("youtu")
}
#[inline] fn progressive(u: &str) -> bool {
    Url::parse(u)
        .ok()
        .and_then(|p| p.path_segments()?.last().map(|s| s.to_ascii_lowercase()))
        .map_or(false, |s| s.ends_with(".mp3") || s.ends_with(".m4a") || s.ends_with(".mp4"))
}

/* ─────────────── 入口 ─────────────── */
pub async fn play_track_req(
    gid: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: PlayingMap,
    tr: TrackRequest,
) -> Result<(TrackHandle, TrackRequest), Error> {
    let handler = TrackEndHandler { guild_id: gid, queues, call: call.clone(), playing: playing.clone() };
    let (h, req) = play_track(call, tr, Some(handler)).await?;
    playing.insert(gid, (h.clone(), req.clone()));
    Ok((h, req))
}

/* ─────────────── 本体 ─────────────── */
pub async fn play_track(
    call: Arc<Mutex<Call>>,
    mut tr: TrackRequest,
    on_end: Option<TrackEndHandler>,
) -> Result<(TrackHandle, TrackRequest), Error> {
    let url = tr.url.clone();

    /* ① YouTube */
    let input: Input = if is_youtube(&url) {
        let mut ytdl = YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), url.clone());
        let audio = ytdl.create_async().await.map_err(Error::from)?;
        tr.meta = ytdl.aux_metadata().await.map_err(Error::from)?;
        Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl)))
    }
    /* ② 直接 URL */
    else if Url::parse(&url).is_ok() {
        let direct = resolve_once(&url).await?;                     // ★ 直リンク
        let referer = Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|h| format!("https://{h}/")))
            .unwrap_or_default();

        let mut hdr = HeaderMap::new();
        if !referer.is_empty() {
            hdr.insert("Referer", HeaderValue::from_str(&referer)?);
        }

        if progressive(&direct) {
            tr.meta.title.get_or_insert_with(|| direct.clone());
            HttpRequest::new_with_headers(get_http_client(), direct.into(), hdr).into()
        } else if direct.contains(".m3u8") && hls_ok(&direct, &referer).await? {
            tr.meta.title.get_or_insert_with(|| direct.clone());
            HlsRequest::new_with_headers(get_http_client(), direct.into(), hdr).into()
        } else {
            return Err(Error::from("unsupported stream (needs ffmpeg)"));
        }
    }
    /* ③ 検索 → YouTube */
    else {
        let mut ytdl = YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), url.clone());
        let audio = ytdl.create_async().await.map_err(Error::from)?;
        tr.meta = ytdl.aux_metadata().await.map_err(Error::from)?;
        Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl)))
    };

    /* 再生 */
    let h = { call.lock().await.play_only(Track::from(input)) };
    if let Some(ev) = on_end {
        h.add_event(Event::Track(TrackEvent::End), ev.clone()).ok();
        h.add_event(Event::Track(TrackEvent::Error), ev).ok();
    }
    Ok((h, tr))
}

/* ─────────────── m3u8 判定 ─────────────── */
async fn hls_ok(m3u: &str, referer: &str) -> Result<bool, Error> {
    let txt = get_http_client()
        .get(m3u)
        .header("Referer", referer)
        .timeout(Duration::from_secs(3))
        .send().await?
        .text().await?;
    Ok(txt.lines().any(|l| l.contains(".ts") || l.contains(".mp4")))
}

/* ─────────────── yt-dlp 一発 ─────────────── */
#[derive(Deserialize)]
struct Fmt { format_id: String, ext: String, url: String, height: Option<u32>, vcodec: Option<String> }

async fn resolve_once(q: &str) -> Result<String, Error> {
    let out = Command::new("yt-dlp")
        .args(["--dump-single-json", "--cookies-from-browser", "firefox", "-f", "best,bestaudio,bestvideo", q])
        .stderr(Stdio::null()).stdout(Stdio::piped())
        .spawn()?.wait_with_output().await?;
    if !out.status.success() { return Err(Error::from("yt-dlp failed")); }

    let fmts: Vec<Fmt> = serde_json::from_slice::<serde_json::Value>(&out.stdout)?
        ["formats"].as_array().ok_or("no formats")?
        .iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect();

    let pick = [
        |f: &Fmt| f.ext == "m4a" && f.vcodec.as_deref() == Some("none"),
        |f: &Fmt| f.ext == "mp3" && !f.url.contains(".m3u8"),
        |f: &Fmt| f.ext == "mp4" && f.height.unwrap_or(0) <= 360 && !f.url.contains("/hls/"),
        |f: &Fmt| f.format_id.starts_with("hls-"),
    ]
    .iter()
    .find_map(|p| fmts.iter().find(|f| p(f)).map(|f| f.url.clone()))
    .ok_or_else(|| Error::from("no suitable format"))?;

    Ok(pick)
}