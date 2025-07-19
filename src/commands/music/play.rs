use songbird::tracks::PlayMode;

use crate::{
    Error,
    commands::music::join::_join,
    util::{alias::Context, play::play_track_req, track::TrackRequest},
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[rest] #[description = "YouTube URL または検索語 (空で再開)"] query: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    /* --- VC 接続 ---------------------------------------------------- */
    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    _join(&ctx, gid, None).await?;
    let call = songbird::get(ctx.serenity_context())
        .await
        .and_then(|m| m.get(gid))
        .ok_or("❌ VC に接続していません")?
        .clone();

    /* --- 共有データ複製 -------------------------------------------- */
    let queues  = ctx.data().queues.clone();
    let playing = ctx.data().playing.clone();
    let http    = ctx.serenity_context().http.clone();
    let ch      = ctx.channel_id();
    let author  = ctx.author().id;

    /* --- 非同期タスク ---------------------------------------------- */
    tokio::spawn(async move {
        /* 0) /play だけ → 一時停止トラックを再開 */
        if query.is_none() {
            if let Some(h) = playing.get_mut(&gid) {
                if matches!(h.value().0.get_info().await.ok().map(|i| i.playing), Some(PlayMode::Pause))
                    && h.value().0.play().is_ok()
                {
                    let _ = ch.say(&http, "▶️ 再開しました").await;
                    return;
                }
            }
        }

        /* 1) クエリがあればキューへ追加 */
        if let Some(q) = &query {
            match TrackRequest::from_url(q.clone(), author).await {
                Ok(req) => { queues.entry(gid).or_default().push_back(req); }
                Err(e)  => { let _ = ch.say(&http, format!("❌ {}", e)).await; return; }
            }
        }

        /* 2) すでに再生中なら終了 */
        let is_playing = if let Some(h) = playing.get(&gid) {
            h.value().0.get_info().await.ok()
                .map(|i| !matches!(i.playing, PlayMode::End | PlayMode::Pause))
                .unwrap_or(false)
        } else {
            false
        };
        
        if is_playing {
            if query.is_some() {
                let _ = ch.say(&http, "🎶 再生中です。キューに追加しました").await;
            }
            return;
        }

        /* 3) 次曲を再生 */
        if let Some(next) = queues.get_mut(&gid).and_then(|mut q| q.pop_next()) {
            match play_track_req(gid, call, queues.clone(), playing.clone(), next).await {
                Ok(_)  => { let _ = ch.say(&http, "▶️ 再生を開始しました").await; }
                Err(e) => { let _ = ch.say(&http, format!("❌ {}", e)).await; }
            }
        } else {
            let _ = ch.say(&http, "❌ キューに曲がありません").await;
        }
    });

    Ok(())
}