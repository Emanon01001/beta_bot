use crate::util::{
    alias::{Context, Error},
    repeat::RepeatMode,
};

#[poise::command(slash_command, guild_only)]
pub async fn repeat(
    ctx: Context<'_>,
    #[description = "Off / Track / Queue"] mode: RepeatMode,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let mut entry = ctx.data().queues.entry(guild_id).or_default();
    entry.value_mut().set_repeat_mode(mode);                    // ← 変更はここだけ
    ctx.say(format!("🔁 リピートモードを **{mode:?}** に設定しました")).await?;
    Ok(())
}