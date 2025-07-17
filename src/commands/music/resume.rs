use songbird::tracks::PlayMode;

use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let playing  = ctx.data().playing.clone();

    let Some(handle_ref) = playing.get(&guild_id) else {
        ctx.say("▶️ 再開できる曲がありません").await?;
        return Ok(());
    };

    match handle_ref.get_info().await {
        Ok(info) if matches!(info.playing, PlayMode::Pause) => {
            handle_ref.play()?;
            ctx.say("▶️ 再生を再開しました").await?;
        }
        _ => {
            ctx.say("曲は一時停止していません").await?;
        },
    }
    Ok(())
}
