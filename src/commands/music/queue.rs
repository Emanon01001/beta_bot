use crate::util::alias::{Context, Error};

use poise::{CreateReply, serenity_prelude::builder::CreateEmbed};

#[poise::command(slash_command, guild_only)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let queues = ctx.data().queues.clone();

    let list = queues
        .get(&guild_id)
        .map(|r| r.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    if list.is_empty() {
        ctx.say("🎵 キューは空です").await?;
        return Ok(());
    }

    // Embed を使うパターン（見た目すっきり）
    let mut embed = CreateEmbed::default();
    embed = embed.title("📋 現在のキュー一覧");

    for (i, tr) in list.iter().enumerate() {
        // --- 1. タイトル or URL ---
        let title = tr.meta.title.as_deref().unwrap_or(&tr.url);

        // --- 3. リクエスター (ユーザーID → メンション) ---
        let requester = format!("<@{}>", tr.requested_by);

        // フィールドとして追加
        embed = embed.field(
            format!("{}. {}", i + 1, title),
            format!("▶️ {}  •  🔗 {}", requester, tr.url),
            false,
        );
    }

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}
