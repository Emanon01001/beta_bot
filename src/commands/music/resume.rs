use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command can only be used in a server")?;

    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird Voice client is not initialized")?;

    let handler_lock = manager
        .get(guild_id)
        .ok_or("❌ Not connected to a voice channel")?
        .clone();

    let call = handler_lock.lock().await;

    if let Some(track) = call.queue().current() {
        track.play()?;
        ctx.say("▶️ Resumed!").await?;
    } else {
        ctx.say("No track is paused!").await?;
    }
    Ok(())
}
