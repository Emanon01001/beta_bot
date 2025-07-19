use crate::util::alias::{Context, Error};
use songbird::tracks::PlayMode;

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?; // ← 3秒ルール

    let guild_id = ctx.guild_id().unwrap();
    let playing = ctx.data().playing.clone();

    let entry = playing
        .get(&guild_id)
        .ok_or(Error::from("再生中の曲がありません"))?;
    let (handle, _req) = entry.value();

    // 2) まだ Playing か確認して pause
    match handle.get_info().await {
        Ok(info) if matches!(info.playing, PlayMode::Play) => {
            handle.pause()?;
            ctx.say("⏸️ 一時停止しました").await?;
        }
        _ => {
            ctx.say("曲はすでに一時停止しています").await?;
        }
    }
    Ok(())
}
