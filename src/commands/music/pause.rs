use crate::util::alias::{Context, Error};
use songbird::tracks::PlayMode;

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?; // ← 3秒ルール

    let guild_id = ctx.guild_id().unwrap();
    let playing = ctx.data().playing.clone();

    // 1) 再生中ハンドルを取得
    let Some(handle_ref) = playing.get(&guild_id) else {
        ctx.say("⏸️ 再生中の曲がありません").await?;
        return Ok(());
    };

    // 2) まだ Playing か確認して pause
    match handle_ref.get_info().await {
        Ok(info) if matches!(info.playing, PlayMode::Play) => {
            handle_ref.pause()?;
            ctx.say("⏸️ 一時停止しました").await?;
        }
        _ => {
            ctx.say("曲はすでに一時停止しています").await?;
        }
    }
    Ok(())
}
