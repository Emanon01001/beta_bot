use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::{
    input::{Compose, LiveInput, YoutubeDl}, tracks::{Track, TrackHandle}, Call, Event, TrackEvent
};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::{
    get_http_client, handlers::track_end::TrackEndHandler, util::{queue::MusicQueue, track::TrackRequest}, Error
};

/// ラッパー: 再生を開始し TrackHandle を返す
pub async fn play_track_req(
    guild_id: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    track_req: TrackRequest,
) -> Result<TrackHandle, Error> {
    let on_end = TrackEndHandler {
        guild_id,
        queues: queues.clone(),
        call: call.clone(),
    };

    // 再生本体へ委譲
    play_track(call, track_req, Some(on_end)).await
}
pub async fn play_track(
    call: Arc<Mutex<Call>>,
    track_req: TrackRequest,
    on_end: Option<TrackEndHandler>,
) -> Result<TrackHandle, Error> {
    // URL をクローン（所有権を渡す）
    let url = track_req.url.clone();

    // --- 入力準備 ---
    let mut ytdl = if Url::parse(&url).is_ok() {
        YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), url)
    } else {
        YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), url)
    };

    let audio = ytdl.create_async().await.map_err(Error::from)?;

    // 以降 track_req.meta の更新が不要なら触らなくてOK
    // ...

    let input = songbird::input::Input::Live(
        LiveInput::Raw(audio),
        Some(Box::new(ytdl)),
    );
    let handle = {
        let mut guard = call.lock().await;
        guard.play_only(Track::from(input))
    };

    if let Some(handler) = on_end {
        handle.add_event(Event::Track(TrackEvent::End), handler).ok();
    }

    Ok(handle)
}
