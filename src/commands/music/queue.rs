use crate::util::track::TrackRequest;
use crate::{Error, util::alias::Context};
use chrono::Utc;
use poise::CreateReply;
use poise::serenity_prelude::{Colour, CreateEmbed};

#[poise::command(slash_command, guild_only)]
pub async fn queue(
    ctx: Context<'_>,
    #[rest]
    #[description = "YouTube URL または検索語 (指定するとキューに追加)"]
    query: Option<String>,
) -> Result<(), Error> {
    // 共通準備
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let queues = ctx.data().queues.clone();

    // 追加モード
    if let Some(q) = query {
        ctx.defer().await?;
        // リクエスト生成
        match TrackRequest::from_url(q.clone(), ctx.author().id).await {
            Ok(req) => {
                queues.entry(guild_id).or_default().push_back(req.clone());
                let title = req.meta.title.clone().unwrap_or(req.url.clone());
                ctx.say(format!("🎶 キューに追加しました: {}", title))
                    .await?;
            }
            Err(e) => {
                ctx.say(format!("❌ 追加に失敗しました: {}", e)).await?;
            }
        }
        return Ok(());
    }

    // 表示モード
    let list = queues
        .get(&guild_id)
        .map(|q| q.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    if list.is_empty() {
        ctx.say("🎵 キューは空です").await?;
        return Ok(());
    }

    // Embed のベース
    let mut embed = CreateEmbed::default();
    embed = embed.title("📋 現在のキュー一覧");
    embed = embed.colour(Colour::BLITZ_BLUE);
    embed = embed.timestamp(Utc::now());
    embed = embed.description(format!("📝 **Total Tracks:** {}", list.len()));

    // いま再生中
    if let Some(now) = list.get(0) {
        let title = now.meta.title.as_deref().unwrap_or(&now.url);
        let link = now.meta.source_url.as_deref().unwrap_or(&now.url);
        let dur = now
            .meta
            .duration
            .map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
            .unwrap_or_else(|| "--:--".into());
        embed = embed.field(
            "▶️ Now Playing",
            format!("[{}]({}) • ⏱️ {}", title, link, dur),
            false,
        );
    }

    // Up Next
    if list.len() > 1 {
        let mut upcoming = String::new();
        for (i, tr) in list.iter().skip(1).enumerate().take(10) {
            let idx = i + 1;
            let title = tr.meta.title.as_deref().unwrap_or(&tr.url);
            let link = tr.meta.source_url.as_deref().unwrap_or(&tr.url);
            let dur = tr
                .meta
                .duration
                .map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
                .unwrap_or_else(|| "--:--".into());
            upcoming.push_str(&format!("{}. [{}]({}) • ⏱️ {}\n", idx, title, link, dur));
        }
        embed = embed.field("⏭️ Up Next", upcoming, false);
    }

    // Embed を送信（クロージャー不使用）
    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}
