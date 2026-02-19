use crate::util::{
    alias::{Context, Error},
    lavalink_player::pause_current_lavalink,
    player::PlaybackControlResult,
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?; // ← 3秒ルール

    let guild_id = ctx.guild_id().unwrap();
    let lavalink = ctx
        .data()
        .lavalink
        .clone()
        .ok_or("Lavalink is not enabled in configuration")?;
    let playing = ctx.data().lavalink_playing.clone();

    match pause_current_lavalink(&lavalink, guild_id, &playing).await? {
        PlaybackControlResult::Changed(_) => {
            ctx.say("⏸️ 一時停止しました").await?;
        }
        PlaybackControlResult::Unchanged => {
            ctx.say("曲はすでに一時停止しています").await?;
        }
        PlaybackControlResult::Missing => {
            ctx.say("再生中の曲がありません").await?;
        }
    }
    Ok(())
}
