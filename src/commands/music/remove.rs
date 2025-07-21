// src/commands/music/remove.rs
use crate::{Error, util::alias::Context};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "キューの位置 (1〜)"] index: usize,
) -> Result<(), Error> {
    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let mut entry = ctx.data().queues.entry(gid).or_default();
    let queue = entry.value_mut();

    if index == 0 || index > queue.len() {
        ctx.reply(format!("❌ 有効な範囲は 1〜{} です", queue.len())).await?;
        return Ok(());
    }

    // 0-based に換算して削除
    if let Some(tr) = queue.remove_at(index - 1) {
        let title = tr.meta.title.as_deref().unwrap_or("Unknown Title");
        ctx.reply(format!("🗑️ キューから削除しました: **{}**", title)).await?;
    } else {
        ctx.reply("❌ 削除に失敗しました").await?;
    }
    Ok(())
}