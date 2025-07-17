use songbird::{
    Call, Event, TrackEvent,
    input::{Compose, LiveInput, YoutubeDl},
    tracks::{Track, TrackHandle},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::{
    Error, get_http_client,
    handlers::track_end::TrackEndHandler,
    util::{queue::MusicQueue, track::TrackRequest},
};

/// Poise から呼び出すラッパー：再生＋通知用
pub async fn play_track_req(
    call: Arc<Mutex<Call>>,
    queue: Arc<Mutex<MusicQueue>>,
    track_req: TrackRequest,
) -> Result<TrackHandle, Error> {
    let on_end = TrackEndHandler {
        queue: queue.clone(),
        call: call.clone(),
    };
    // 純粋再生関数を呼び出し
    let handle: TrackHandle = play_track(call, track_req.clone(), Some(on_end)).await?;

    // タイトルを返す
    Ok(handle)
}

/// Poise依存なし「再生だけ」を担当
pub async fn play_track(
    call: Arc<Mutex<Call>>,
    mut track_req: TrackRequest,
    on_end: Option<TrackEndHandler>,
) -> Result<TrackHandle, Error> {
    // 1) 入力準備
    let mut ytdl = if Url::parse(&track_req.url).is_ok() {
        YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), track_req.url)
    } else {
        YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), track_req.url)
    };
    let audio = ytdl.create_async().await.map_err(Error::from)?;
    let meta = ytdl.aux_metadata().await.unwrap_or_default();
    track_req.meta = meta.clone();

    // 2) Track化
    let input = songbird::input::Input::Live(LiveInput::Raw(audio), Some(Box::new(ytdl)));
    let track = Track::from(input);

    // 3) 再生（必要ならstopは呼び出し元で制御してもOK）
    let handle = {
        let mut guard = call.lock().await;
        // guard.stop(); // 割り込み再生したい場合のみ有効化
        guard.play_only(track)
    };

    // 4) 終了イベント
    if let Some(handler) = on_end {
        handle
            .add_event(Event::Track(TrackEvent::End), handler)
            .ok();
    }

    Ok(handle)
}
