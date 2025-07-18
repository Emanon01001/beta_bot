use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command, guild_only)]
/// シャッフルモードを切り替えます
pub async fn shuffle(ctx: Context<'_>, #[description = "シャッフルモードを切り替えます"] option: bool) -> Result<(), Error> {
    ctx.defer().await?; // 3秒ルール
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let mut q = ctx.data().queues.entry(guild_id).or_default();
    q.value_mut().set_shuffle(option); // シャッフルモードの設定
    let status = if option { "ON" } else { "OFF" };
    ctx.say(format!("🔀 シャッフル再生を **{status}** にしました")).await?;
    Ok(())
}