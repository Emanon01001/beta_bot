use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー限定コマンドです")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird Voice client is not initialized")?;
    let handler_lock = manager
        .get(guild_id)
        .ok_or("❌ Not connected to a voice channel")?
        .clone();

    handler_lock.lock().await.stop();

    ctx.data().music.lock().await.clear();

    ctx.say("⏹️ Stopped all playback").await?;
    Ok(())
}
