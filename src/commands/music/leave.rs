use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?; // 3秒ルール
    let guild_id = ctx.guild_id().unwrap();
    let manager = songbird::get(ctx.serenity_context()).await.unwrap();
    let call = manager
        .get(guild_id)
        .ok_or("❌ Not connected to a voice channel")?;
    call.lock().await.leave().await?;
    ctx.say("✅ Left the voice channel").await?;
    Ok(())
}
