use crate::{Error, get_http_client, util::alias::Context};
use poise::builtins::paginate;
use songbird::input::{AuxMetadata, YoutubeDl};

const PAGE_SIZE: usize = 5;
const MAX_RESULTS: usize = 50;

#[poise::command(slash_command, guild_only)]
pub async fn search(
    ctx: Context<'_>,
    #[rest]
    #[description = "検索キーワード"]
    query: String,
    #[description = "取得件数(1-50)"]
    count: Option<usize>,
) -> Result<(), Error> {
    // 1) 検索中フィードバック
    ctx.defer().await?;

    // 2) 件数調整＆yt-dlp flat-playlist 実行
    let n = count.unwrap_or(5).clamp(1, MAX_RESULTS);
    let mut ytdl =
        YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), query.clone())
            .user_args(vec!["--flat-playlist".into(), "--dump-json".into()]);
    let metas: Vec<AuxMetadata> = ytdl.search(Some(n)).await?.take(n).collect();

    // 3) 結果なしチェック
    if metas.is_empty() {
        ctx.say("❌ 結果が見つかりませんでした").await?;
        return Ok(());
    }

    // 4) テキストページを作成
    let page_texts: Vec<String> = metas
        .chunks(PAGE_SIZE)
        .enumerate()
        .map(|(pi, chunk)| {
            let mut txt = format!(
                "🔎 『{}』の検索結果 ({}/{})\n\n",
                query,
                pi + 1,
                (n + PAGE_SIZE - 1) / PAGE_SIZE
            );
            for (i, meta) in chunk.iter().enumerate() {
                let idx = pi * PAGE_SIZE + i + 1;
                let title = meta.title.as_deref().unwrap_or("Unknown");
                let url = meta.source_url.as_deref().unwrap_or("-");
                let dur = meta
                    .duration
                    .map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
                    .unwrap_or_else(|| "??:??".into());
                txt.push_str(&format!(
                    "{}. **{}**\n▶️ {}\n⏱️ {}\n\n",
                    idx, title, url, dur
                ));
            }
            txt
        })
        .collect();

    // 5) Vec<String> → &[&str] に変換
    let page_slices: Vec<&str> = page_texts.iter().map(String::as_str).collect();

    // 6) paginate を呼び出し
    paginate(ctx, &page_slices).await?;

    Ok(())
}