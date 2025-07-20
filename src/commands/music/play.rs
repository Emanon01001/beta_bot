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
    let m = ctx.say("⏱️ 準備中…").await?.into_message().await?;

    // --- VC 接続 & Call 取得 ------------------------------------
    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    _join(&ctx, gid, None).await?;
    let call = songbird::get(ctx.serenity_context())
        .await
        .and_then(|m| m.get(gid))
        .ok_or("❌ VC に接続していません")?
        .clone();

    // --- Data & クローン ----------------------------------------
    let queues  = ctx.data().queues.clone();
    let playing = ctx.data().playing.clone();
    let http    = ctx.serenity_context().http.clone();
    let ch      = ctx.channel_id();
    let author  = ctx.author().id;
    let query   = query.clone(); // spawn 内で ownership が必要

    tokio::spawn(async move {
        // --- 現在の再生状態を取得 ---------------------------------
        let current_handle = playing.get(&gid).map(|e| e.value().0.clone());
        let current_state = if let Some(handle) = &current_handle {
            handle
                .get_info()
                .await
                .map(|info| info.playing)
                .unwrap_or(PlayMode::Stop)
        } else {
            PlayMode::Stop
        };

        // 0) クエリなしで一時停止中なら再開
        if query.is_none() && current_state == PlayMode::Pause {
            if let Some(handle) = current_handle {
                let _ = handle.play();
                let _ = ch.say(&http, "▶️ 再開しました").await;
                let _ = http.delete_message(ch, m.id, None).await;
                return;
            }
        }

        // 1) クエリありなら → リクエスト作成 & 再生 or キュー追加
        if let Some(q) = query {
            match TrackRequest::from_url(q, author).await {
                Ok(req) => {
                    if current_state == PlayMode::Play {
                        // 再生中ならキューに積む
                        queues.entry(gid).or_default().push_back(req.clone());
                        let _ = ch.say(&http, "🎶 キューに追加しました").await;
                    } else {
                        // 再生中でなければ即再生
                        match play_track_req(
                            gid,
                            call.clone(),
                            queues.clone(),
                            playing.clone(),
                            req.clone(),
                        )
                        .await
                        {
                            Ok((h, _)) => {
                                playing.insert(gid, (h, req));
                                let _ = ch.say(&http, "▶️ 再生を開始しました").await;
                            }
                            Err(e) => {
                                let _ = ch.say(&http, format!("❌ {}", e)).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = ch.say(&http, format!("❌ {}", e)).await;
                }
            }
            let _ = http.delete_message(ch, m.id, None).await;
            return;
        }

        // 2) クエリなし & 再生中でないなら → キューから次曲再生
        if current_state != PlayMode::Play {
            playing.remove(&gid); // 古いハンドルを掃除

            if let Some(next_req) = queues.get_mut(&gid).and_then(|mut q| q.pop_next()) {
                match play_track_req(
                    gid,
                    call.clone(),
                    queues.clone(),
                    playing.clone(),
                    next_req.clone(),
                )
                .await
                {
                    Ok((h, _)) => {
                        playing.insert(gid, (h, next_req));
                        let _ = ch.say(&http, "▶️ 次の曲を再生しました").await;
                    }
                    Err(e) => {
                        let _ = ch.say(&http, format!("❌ {}", e)).await;
                    }
                }
            } else {
                let _ = ch.say(&http, "❌ キューに曲がありません").await;
            }
            let _ = http.delete_message(ch, m.id, None).await;
        }
        // 既に再生中なら何もしない
    });

    Ok(())
}