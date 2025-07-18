use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    // VC handle 取得
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird 未初期化")?;
    let call = manager
        .get(guild_id)
        .ok_or("❌ VC に接続していません")?
        .clone();

    // 停止
    call.lock().await.stop();
    // キューもクリア
    ctx.data().queues.remove(&guild_id);
    ctx.data().playing.remove(&guild_id);

    ctx.say("⏹️ 再生を停止し、キューをクリアしました").await?;
    Ok(())
}
