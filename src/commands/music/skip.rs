use crate::util::{
    alias::{Context, Error},
    play::play_track_req,
};

#[poise::command(slash_command, prefix_command)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    // 1) join／VoiceClient の取得は要るならここで
    let guild_id = ctx.guild_id().ok_or("❌ サーバー内で実行してください")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird 未初期化")?;
    let call = manager
        .get(guild_id)
        .ok_or("❌ VC に接続していません")?
        .clone();

    let queue = ctx.data().music.clone();
    let next_req_opt = {
        let mut q = queue.lock().await;
        q.pop_next() // あなたの pop() → Option<TrackRequest>
    };

    if next_req_opt.is_some() {
        ctx.say("⏭️ スキップしました！次の曲を再生します…").await?;
    } else {
        ctx.say("❌ スキップできる曲がキューにありません").await?;
        return Ok(());
    }

    let call2 = call.clone();
    let queue2 = queue.clone();
    tokio::spawn(async move {
        if let Some(next_req) = next_req_opt {
            // play_track_req は TrackHandle を返すようにしてある想定
            if let Err(err) = play_track_req(call2, queue2, next_req).await {
                tracing::error!("skip: play_track_req error: {:?}", err);
            }
        }
    });
    Ok(())
}
