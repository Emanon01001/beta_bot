use songbird::tracks::PlayMode;

use crate::{
    Error,
    commands::music::join::_join,
    util::{alias::Context, play::play_track_req, track::TrackRequest},
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[rest]
    #[description = "YouTube URL または検索語 (空で再開)"]
    query: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let m = ctx.say("⏱️ 再生準備中…").await?.into_message().await?; // ② フォローアップを返してタイマー解除

    /* --- VC 接続 & Call 取得 ------------------------------------ */
    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    _join(&ctx, gid, None).await?;
    let call = songbird::get(ctx.serenity_context())
        .await
        .and_then(|m| m.get(gid))
        .ok_or("❌ VC に接続していません")?
        .clone();

    /* --- Data & HTTP/Channel/Author クローン ------------------- */
    let queues  = ctx.data().queues.clone();
    let playing = ctx.data().playing.clone();
    let http    = ctx.serenity_context().http.clone();
    let ch      = ctx.channel_id();
    let author  = ctx.author().id;

    tokio::spawn(async move {
        // 0) クエリなしなら「一時停止中トラック」を再開
        if query.is_none() {
            if let Some(entry) = playing.get(&gid) {
                let handle = entry.value().0.clone();
                if let Ok(info) = handle.get_info().await {
                    if matches!(info.playing, PlayMode::Pause) && handle.play().is_ok() {
                        let _ = ch.say(&http, "▶️ 再開しました").await;
                        return;
                    }
                }
            }
        }

        // 1) クエリがあればキューに追加
        if let Some(q) = &query {
            match TrackRequest::from_url(q.clone(), author).await {
                Ok(req) => {
                    queues.entry(gid).or_default().push_back(req);
                    let _ = ch.say(&http, "🎶 キューに追加しました").await;
                    http.delete_message(ch, m.id, None).await.ok();
                }
                Err(e) => {
                    let _ = ch.say(&http, format!("❌ {}", e)).await;
                    http.delete_message(ch, m.id, None).await.ok();
                    return;
                }
            }
        }

        // 3) 次曲を取り出して再生
        if let Some(next_req) = queues.get_mut(&gid).and_then(|mut q| q.pop_next()) {
            match play_track_req(gid, call.clone(), queues.clone(), playing.clone(), next_req.clone()).await {
                Ok((h, _)) => {
                    playing.insert(gid, (h.clone(), next_req));
                    let _ = ch.say(&http, "▶️ 再生を開始しました").await;
                    http.delete_message(ch, m.id, None).await.ok();

                }
                Err(e) => {
                    let _ = ch.say(&http, format!("❌ {}", e)).await;
                    http.delete_message(ch, m.id, None).await.ok();
                }
            }
        } else {
            let _ = ch.say(&http, "❌ キューに曲がありません").await;
            http.delete_message(ch, m.id, None).await.ok();
        }
    });

    Ok(())
}