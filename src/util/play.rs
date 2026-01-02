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
    process::Command,
    sync::Mutex,
    time::{Duration, timeout},
};

use crate::{
    Error, get_http_client,
    handlers::track_end::TrackEndHandler,
    util::{
        queue::MusicQueue,
        track::TrackRequest,
        types::{HistoryMap, NowPlayingMap, PlayingMap, TransitionFlags},
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
    transition_flags: TransitionFlags,
    history: HistoryMap,
    http: Arc<poise::serenity_prelude::Http>,
    now_playing: NowPlayingMap,
    tr: TrackRequest,
) -> Result<(TrackHandle, TrackRequest), Error> {
    tracing::info!(guild = %gid, url = %tr.url, "Play request");
    let handler = TrackEndHandler {
        guild_id: gid,
        queues,
        call: call.clone(),
        playing: playing.clone(),
        transition_flags,
        history: history.clone(),
        http,
        now_playing,
    };

    let (h, req) = play_track(call, tr, Some(handler)).await?;
    playing.insert(gid, (h.clone(), req.clone()));
    {
        const HISTORY_MAX: usize = 50;
        let mut h = history.entry(gid).or_default();
        h.push_back(req.clone());
        while h.len() > HISTORY_MAX {
            h.pop_front();
        }
    }
    Ok((h, req))
}

pub struct PlayNextResult {
    pub started: Option<TrackRequest>,
    pub skipped: usize,
    pub remaining: usize,
    pub last_error: Option<String>,
}

pub async fn play_next_from_queue(
    gid: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: PlayingMap,
    transition_flags: TransitionFlags,
    history: HistoryMap,
    http: Arc<poise::serenity_prelude::Http>,
    now_playing: NowPlayingMap,
    max_attempts: usize,
) -> Result<PlayNextResult, Error> {
    let mut skipped = 0usize;
    let mut last_error: Option<String> = None;
    let mut remaining = 0usize;

    for _ in 0..max_attempts.max(1) {
        let next_req = if let Some(mut q) = queues.get_mut(&gid) {
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

        match play_track_req(
            gid,
            call.clone(),
            queues.clone(),
            playing.clone(),
            transition_flags.clone(),
            history.clone(),
            http.clone(),
            now_playing.clone(),
            req,
        )
        .await
        {
            Ok((_handle, started_req)) => {
                return Ok(PlayNextResult {
                    started: Some(started_req),
                    skipped,
                    remaining,
                    last_error,
                });
            }
            Err(e) => {
                last_error = Some(e.to_string());
                skipped += 1;
                continue;
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

/// Run a lightweight yt-dlp command to surface stderr when playback setup fails.
async fn yt_dlp_diagnostics(url: &str, use_android_client: bool) -> Option<String> {
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-4")
        .arg("--ignore-config")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--geo-bypass")
        .arg("-f")
        .arg("bestaudio[protocol^=http]/bestaudio")
        .arg("-g");
    if use_android_client {
        cmd.arg("--extractor-args")
            .arg("youtube:player_client=android");
    }
    cmd.args(cookies_args());
    cmd.args(extra_args_from_config());
    cmd.arg(url);

    match timeout(Duration::from_secs(10), cmd.output()).await {
        Ok(Ok(out)) => {
            if out.status.success() {
                return None;
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let detail = if !stderr.trim().is_empty() {
                stderr.lines().take(3).collect::<Vec<_>>().join(" | ")
            } else if !stdout.trim().is_empty() {
                format!(
                    "stdout: {}",
                    stdout.lines().take(2).collect::<Vec<_>>().join(" | ")
                )
            } else {
                format!("exit status {}", out.status)
            };
            Some(detail.chars().take(500).collect())
        }
        Ok(Err(e)) => Some(format!("yt-dlp spawn error: {e}")),
        Err(_) => Some("yt-dlp diagnostic timed out".into()),
    }
}

async fn youtube_input(
    tr: &mut TrackRequest,
    use_android_client: bool,
) -> Result<Input, String> {
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
    if use_android_client {
        ytdl = ytdl.user_args(vec![
            "--extractor-args".into(),
            "youtube:player_client=android".into(),
        ]);
    }

    if let Ok(meta) = ytdl.aux_metadata().await {
        tr.meta = meta;
        if let Some(src) = tr.meta.source_url.clone() {
            tr.url = src;
        }
    }
    let fut = ytdl.create_async();
    let audio = match timeout(Duration::from_secs(20), fut).await {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("yt-dlp (YouTube) がタイムアウトしました".into()),
    };
    Ok(Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl))))
}

/// yt-dlp を使って入力をストリームに変換し、メタデータを可能な範囲で保持する。
async fn resolve_input(tr: &mut TrackRequest) -> Result<Input, Error> {
    fn is_403_forbidden(s: &str) -> bool {
        s.contains("403") && s.contains("Forbidden")
    }

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
        let primary_err = match youtube_input(tr, false).await {
            Ok(input) => return Ok(input),
            Err(e) => e,
        };
        tracing::warn!(
            url = %tr.url,
            error = %primary_err,
            "yt-dlp (YouTube) first attempt failed; retrying with android client"
        );

        match youtube_input(tr, true).await {
            Ok(input) => return Ok(input),
            Err(second_err) => {
                let needs_diag = primary_err.contains("<no error message>")
                    || second_err.contains("<no error message>")
                    || primary_err.trim().is_empty()
                    || second_err.trim().is_empty();
                let diag = if needs_diag {
                    yt_dlp_diagnostics(&tr.url, true).await
                } else {
                    None
                };

                let mut msg = format!("yt-dlp (YouTube) 実行失敗: {second_err}");
                if second_err != primary_err {
                    msg.push_str(&format!(" (再試行前: {primary_err})"));
                }
                if let Some(d) = diag {
                    tracing::warn!(url = %tr.url, detail = %d, "yt-dlp diagnostic detail");
                    msg.push_str(&format!(" (詳細: {d})"));
                }
                return Err(Error::from(msg));
            }
        }
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
            let msg = e.to_string();
            if is_403_forbidden(&msg) {
                tracing::warn!(error = %msg, "yt-dlp (検索) が 403。player_client=android で再試行します");
                let mut ytdl2 = YoutubeDl::new_search_ytdl_like(
                    "yt-dlp",
                    get_http_client(),
                    tr.url.to_string(),
                )
                .user_args(vec![
                    "-4".into(),
                    "--ignore-config".into(),
                    "--no-warnings".into(),
                    "--no-playlist".into(),
                    "--geo-bypass".into(),
                    "--extractor-args".into(),
                    "youtube:player_client=android".into(),
                    "-f".into(),
                    "bestaudio[protocol^=http]/bestaudio".into(),
                ])
                .user_args(cookies_args())
                .user_args(extra_args_from_config());
                let fut2 = ytdl2.create_async();
                let audio2 = match timeout(Duration::from_secs(20), fut2).await {
                    Ok(Ok(a2)) => a2,
                    Ok(Err(e2)) => {
                        return Err(Error::from(format!("yt-dlp (検索) 実行失敗: {e2}")));
                    }
                    Err(_) => return Err(Error::from("yt-dlp (検索) がタイムアウトしました")),
                };
                return Ok(Input::Live(LiveInput::Raw(audio2), Some(Box::new(ytdl2))));
            }

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
