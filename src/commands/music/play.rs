use crate::{
    Error,
    commands::music::join::_join,
    util::{alias::Context, play::play_track_req, track::TrackRequest},
};
use songbird::tracks::PlayMode;

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[rest]
    #[description = "YouTube URL または検索語 (空で再開)"]
    query: Option<String>,
) -> Result<(), Error> {
    // --- Defer: thinking を出す（後で ctx.reply() 1 回で必ず置換する） ---
    ctx.defer().await?;

    // --- VC 接続 & Call 取得 -------------------------------------------------
    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    _join(&ctx, gid, None).await?;
    let call = songbird::get(ctx.serenity_context())
        .await
        .and_then(|m| m.get(gid))
        .ok_or("❌ VC に接続していません")?
        .clone();

    let queues = ctx.data().queues.clone();
    let playing = ctx.data().playing.clone();
    let author = ctx.author().id;

    let (current_handle, current_state) = if let Some(entry) = playing.get(&gid) {
        let (handle, _req) = entry.value();
        let state = handle
            .get_info()
            .await
            .map(|info| info.playing)
            .unwrap_or(PlayMode::Stop);
        (Some(handle.clone()), state)
    } else {
        (None, PlayMode::Stop)
    };

    if query.is_none() && current_state == PlayMode::Pause {
        if let Some(h) = current_handle {
            let _ = h.play();
            ctx.reply("▶️ 再開しました").await?;
            return Ok(());
        }
    }

    if let Some(q) = query {
        match TrackRequest::from_url(q, author).await {
            Ok(req) => {
                if current_state == PlayMode::Play {
                    // 再生中 → キューに積むだけ
                    queues.entry(gid).or_default().push_back(req);
                    ctx.reply("🎶 再生中です。キューに追加しました").await?;
                    return Ok(());
                } else {
                    // 再生していない → 即再生
                    match play_track_req(gid, call.clone(), queues.clone(), playing.clone(), req)
                        .await
                    {
                        Ok((_handle, _req)) => {
                            // play_track_req 内で playing.insert 済み
                            ctx.reply("▶️ 再生を開始しました").await?;
                            return Ok(());
                        }
                        Err(e) => {
                            ctx.reply(format!("❌ 再生開始に失敗: {e}")).await?;
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                ctx.reply(format!("❌ リクエスト生成失敗: {e}")).await?;
                return Ok(());
            }
        }
    }

    // 2) クエリなし & 未再生 (Stop / 終了状態) → 次曲をキューから再生
    if current_state != PlayMode::Play {
        // 古い handle の掃除（存在したら）
        playing.remove(&gid);

        if let Some(next_req) = queues.get_mut(&gid).and_then(|mut q| q.pop_next()) {
            match play_track_req(gid, call.clone(), queues.clone(), playing.clone(), next_req).await
            {
                Ok((_handle, _req)) => {
                    ctx.reply("▶️ 次の曲を再生しました").await?;
                    return Ok(());
                }
                Err(e) => {
                    ctx.reply(format!("❌ 次曲再生失敗: {e}")).await?;
                    return Ok(());
                }
            }
        } else {
            ctx.reply("❌ キューに曲がありません").await?;
            return Ok(());
        }
    }

    // 3) ここまで来たら「クエリなし・すでに再生中」ケース
    ctx.reply("🎶 既に再生中です").await?;
    Ok(())
}
