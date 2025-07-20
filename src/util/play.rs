//! play.rs  ―  Tracing を残しつつ最小限の実装に整理
use std::{process::Stdio, sync::Arc, time::Duration};

use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue},
};
use serde::Deserialize;
use songbird::{
    Call, Event, TrackEvent,
    input::{Compose, HlsRequest, HttpRequest, Input, LiveInput, YoutubeDl},
    tracks::{Track, TrackHandle},
};
use tokio::{process::Command, sync::Mutex};
use url::Url;

use crate::{
    Error, get_http_client,
    handlers::track_end::TrackEndHandler,
    util::{queue::MusicQueue, track::TrackRequest, types::PlayingMap},
};

/* ------------------------------------------------- */
/* ヘルパ                                            */
/* ------------------------------------------------- */
fn is_youtube(u: &str) -> bool {
    u.contains("youtube.com") || u.contains("youtu.be") || u.contains("m.youtube.com")
}

fn is_progressive(u: &str) -> bool {
    Url::parse(u)
        .ok()
        .and_then(|p| p.path_segments()?.last().map(|s| s.to_owned()))
        .map(|s| s.split('?').next().unwrap_or(&s).to_ascii_lowercase())
        .map_or(false, |seg| {
            seg.ends_with(".mp3") || seg.ends_with(".m4a") || seg.ends_with(".mp4")
        })
}

fn plain_client() -> Client {
    Client::builder()
        .no_brotli()
        .no_gzip()
        .no_deflate()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap()
}

/* ------------------------------------------------- */
/* エントリ                                          */
/* ------------------------------------------------- */
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

/* ------------------------------------------------- */
/* 本体                                              */
/* ------------------------------------------------- */
pub async fn play_track(
    call: Arc<Mutex<Call>>,
    mut tr: TrackRequest,
    on_end: Option<TrackEndHandler>,
) -> Result<(TrackHandle, TrackRequest), Error> {
    let url = tr.url.clone();
    tracing::info!(%url, "Processing track");

    let input = resolve_input(&url, &mut tr).await?;
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

/* ------------------------------------------------- */
/* 入力解決                                          */
/* ------------------------------------------------- */
async fn resolve_input(url: &str, tr: &mut TrackRequest) -> Result<Input, Error> {
    // 1. YouTube
    if is_youtube(url) {
        tracing::info!("Source: YouTube");
        let mut ytdl = YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), url.to_string());
        let audio = ytdl.create_async().await.map_err(Error::from)?;
        tr.meta = ytdl.aux_metadata().await.map_err(Error::from)?;
        return Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl))));
    }

    // 2. 直接 URL
    if Url::parse(url).is_ok() {
        tracing::info!("Resolving direct URL");
        let direct = resolve_with_single_call(url).await?;
        tracing::info!(resolved = %direct);

        let referer = Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| format!("https://{h}/")))
            .unwrap_or_default();

        let mut hdrs = HeaderMap::new();
        if !referer.is_empty() {
            hdrs.insert("Referer", HeaderValue::from_str(&referer)?);
        }

        return if is_progressive(&direct) {
            if direct.ends_with(".mp4") {
                hdrs.insert("Range", HeaderValue::from_static("bytes=0-"));
            }
            tr.meta.title.get_or_insert_with(|| direct.clone());
            Ok(HttpRequest::new_with_headers(plain_client(), direct.into(), hdrs).into())
        } else if direct.contains(".m3u8") && is_supported_hls(&direct, &referer).await? {
            tracing::info!("Source: HLS");
            tr.meta.title.get_or_insert_with(|| direct.clone());
            Ok(HlsRequest::new_with_headers(get_http_client(), direct.into(), hdrs).into())
        } else {
            Err(Error::from("unsupported stream (needs ffmpeg)"))
        };
    }

    // 3. 検索（yt-dlp の search: に委ねる）
    tracing::info!("Source: yt-dlp search");
    let mut ytdl = YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), url.to_string());
    let audio = ytdl.create_async().await.map_err(Error::from)?;
    tr.meta = ytdl.aux_metadata().await.map_err(Error::from)?;
    Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl))))
}

/* ------------------------------------------------- */
/* yt-dlp ラッパー                                   */
/* ------------------------------------------------- */
#[derive(Deserialize)]
struct YtdlpInfo {
    formats: Vec<FormatInfo>,
}
#[derive(Deserialize)]
struct FormatInfo {
    format_id: String,
    ext: String,
    url: String,
    height: Option<u32>,
    vcodec: Option<String>,
}

async fn resolve_with_single_call(q: &str) -> Result<String, Error> {
    let out = Command::new("yt-dlp")
        .args(["--dump-single-json", "-f", "bestaudio,best", q])
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()
        .await?;
    if !out.status.success() {
        return Err(Error::from("yt-dlp failed"));
    }

    let info: YtdlpInfo = serde_json::from_slice(&out.stdout)?;
    let pick = [
        |f: &FormatInfo| f.ext == "mp3",
        |f: &FormatInfo| f.ext == "m4a" && f.vcodec.as_deref() == Some("none"),
        |f: &FormatInfo| f.ext == "mp4" && f.height.unwrap_or(0) <= 360 && !f.url.contains("/hls/"),
        |f: &FormatInfo| f.format_id.starts_with("hls-"),
    ]
    .iter()
    .find_map(|p| info.formats.iter().find(|f| p(f)).map(|f| f.url.clone()))
    .ok_or_else(|| Error::from("no suitable format"))?;
    Ok(pick)
}

/* ------------------------------------------------- */
/* HLS マニフェスト検査                              */
/* ------------------------------------------------- */
async fn is_supported_hls(manifest: &str, referer: &str) -> Result<bool, Error> {
    let txt = get_http_client()
        .get(manifest)
        .header("Referer", referer)
        .timeout(Duration::from_secs(3))
        .send()
        .await?
        .text()
        .await?;
    Ok(txt
        .lines()
        .any(|l| l.contains(".ts") || l.contains(".m4a") || l.contains(".mp4")))
}
