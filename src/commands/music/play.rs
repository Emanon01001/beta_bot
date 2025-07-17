use songbird::tracks::PlayMode;

use crate::{
    Error,
    commands::music::join::_join,
    util::{alias::Context, play::play_track_req, track::TrackRequest},
};

#[poise::command(slash_command, prefix_command)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "YouTube URL または検索語"]
    #[rest]
    query: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| "このコマンドはサーバー内で実行してください")?;
    _join(&ctx, guild_id, None).await?;

    let queue = ctx.data().music.clone(); // Arc<Mutex<MusicQueue>>
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or_else(|| "Songbird が初期化されていません")?;
    let call = manager
        .get(guild_id)
        .ok_or_else(|| "❌ ボイスチャンネルに接続されていません")?
        .clone(); // Arc<Mutex<Call>>

    let _ = if let Some(req) = query {
        let track_req = TrackRequest::from_url(req, ctx.author().id).await?;
        queue.lock().await.push_back(track_req);
    };

    let mut playing_lock = ctx.data().playing.lock().await;

    if let Some(handle) = &*playing_lock {
        let info = handle.get_info().await.map_err(Error::from)?;
        match info.playing {
            PlayMode::Play => ctx.say("➕Add to the queue").await?,
            PlayMode::Pause => ctx.say("⏸️ Paused").await?,
            PlayMode::Stop => ctx.say("⏹️ Stopped").await?,
            PlayMode::End => ctx.say("🔚 Ended").await?,
            PlayMode::Errored(e) => ctx.say(format!("❌ Error: {}", e)).await?,
            _ => ctx.say("❓ Unknown play mode").await?,
        };
    } else {
        if let Some(next_req) = queue.lock().await.pop_next() {
            let handle = play_track_req(call.clone(), queue.clone(), next_req).await?;
            *playing_lock = Some(handle.clone());
            ctx.say("▶️ 再生を開始しました").await?;
        } else {
            ctx.say("❌ キューに曲がありません").await?;
        }
    }

    Ok(())
}
