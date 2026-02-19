use crate::util::{
    alias::{Context, Error},
    lavalink_player::resume_current_lavalink,
    player::PlaybackControlResult,
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let lavalink = ctx
        .data()
        .lavalink
        .clone()
        .ok_or("Lavalink is not enabled in configuration")?;
    let playing = ctx.data().lavalink_playing.clone();

    match resume_current_lavalink(&lavalink, guild_id, &playing).await? {
        PlaybackControlResult::Changed(_) => {
            ctx.say("▶️ 再生を再開しました").await?;
        }
        PlaybackControlResult::Unchanged => {
            ctx.say("曲は一時停止していません").await?;
        }
        PlaybackControlResult::Missing => {
            ctx.say("再生中の曲がありません").await?;
        }
    }
    Ok(())
}
