use crate::util::alias::{Context, Error};

#[poise::command(slash_command, guild_only)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let queues = ctx.data().queues.clone();

    // そのギルドのキューを読み取り
    let list = queues
        .get(&guild_id)
        .map(|r| r.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    if list.is_empty() {
        ctx.say("🎵 キューは空です").await?;
        return Ok(());
    }

    let mut msg = String::from("📋 現在のキュー一覧:\n");
    for (i, tr) in list.iter().enumerate() {
        let title = tr.meta.title.as_deref().unwrap_or(&tr.url);
        msg.push_str(&format!("{}. {}\n", i + 1, title));
    }
    ctx.say(msg).await?;
    Ok(())
}
