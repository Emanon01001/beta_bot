use crate::util::alias::{Context, Error};
use crate::util::lavalink_player::delete_player;

#[poise::command(slash_command, prefix_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?; // 3秒ルール
    let guild_id = ctx.guild_id().unwrap();
    let lavalink = ctx
        .data()
        .lavalink
        .clone()
        .ok_or("Lavalink is not enabled in configuration")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialised")?;

    delete_player(&lavalink, guild_id).await?;
    ctx.data().lavalink_playing.remove(&guild_id);

    if let Some(call) = manager.get(guild_id) {
        call.lock().await.leave().await?;
    } else {
        return Err("❌ Not connected to a voice channel".into());
    }

    ctx.data().now_playing.remove(&guild_id);

    ctx.say("✅ Left the voice channel").await?;
    Ok(())
}
