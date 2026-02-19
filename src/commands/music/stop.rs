use crate::util::{
    alias::{Context, Error},
    lavalink_player::stop_and_clear_lavalink,
    player::ManualTransitionGuard,
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let _guard = ManualTransitionGuard::acquire(&ctx.data().transition_flags, guild_id);

    if ctx.data().lavalink.is_none() {
        return Err("Lavalink is not enabled in configuration".into());
    }
    stop_and_clear_lavalink(&ctx, guild_id).await?;

    ctx.say("⏹️ 再生を停止し、キューをクリアしました").await?;
    Ok(())
}
