use crate::{Error, get_http_client, util::alias::Context};
use chrono::Utc;
use poise::serenity_prelude::{Colour, CreateEmbed, CreateMessage};
use songbird::input::{AuxMetadata, YoutubeDl};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn search(
    ctx: Context<'_>,
    #[rest]
    #[description = "検索キーワード"]
    query: String,
) -> Result<(), Error> {
    // フィードバック
    ctx.defer().await?;
    let feedback = ctx.say("🔎 検索中…").await?.into_message().await?;

    // flat-playlist モードで起動
    let mut ytdl = YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), query.clone())
        .user_args(vec![
            "--flat-playlist".into(),
            "--dump-json".into(),
            "--default-search".into(),
            "ytsearch5:".into(),
        ]);

    // AuxMetadata を取得
    let metas: Vec<AuxMetadata> = ytdl.search(None).await?.take(5).collect();

    // Embed を組み立て
    let mut embed = CreateEmbed::new()
        .title(format!("🔎 「{}」の検索結果", query))
        .colour(Colour::BLITZ_BLUE)
        .timestamp(Utc::now());

    for (i, meta) in metas.iter().enumerate() {
        let title = meta.title.as_deref().unwrap_or("-");
        let url   = meta.source_url.as_deref().unwrap_or("-");
        // 分:秒形式にフォーマット
        let duration = meta.duration.map(|d| {
            let secs = d.as_secs();
            format!("{:02}:{:02}", secs / 60, secs % 60)
        }).unwrap_or_else(|| "Unknown".into());

        embed = embed.field(
            format!("{}: {}", i + 1, title),
            format!("▶️ {}\n⏱️ {}", url, duration),
            false,
        );
    }

    // 主チャンネルへ送信
    let builder = CreateMessage::new().embed(embed.clone());
    ctx.channel_id()
        .send_message(&ctx.serenity_context().http, builder)
        .await?;

    // フィードバック削除
    let _ = ctx
        .http()
        .delete_message(ctx.channel_id(), feedback.id, None)
        .await;

    Ok(())
}
