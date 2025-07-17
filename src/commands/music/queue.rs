use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    // 1) 応答を defer
    ctx.defer().await?;

    // 2) 自作キューをロックして全要素を Vec にコピー
    let list = {
        let guard = ctx.data().music.lock().await;
        guard.to_vec()
    };

    // 3) 空チェック
    if list.is_empty() {
        ctx.say("🎵 キューは現在空です").await?;
        return Ok(());
    }

    // 4) メッセージ組み立て
    let mut msg = String::from("📋 現在のキュー一覧:\n");
    for (i, tr) in list.iter().enumerate() {
        // タイトルがあれば表示、なければ URL
        let title = tr.meta.title.as_deref().unwrap_or(&tr.url);
        // リクエスト者のメンション
        let user = format!("<@{}>", tr.requested_by);
        msg.push_str(&format!(
            "**{}**. {} — リクエスト: {}\n",
            i + 1,
            title,
            user
        ));
    }

    // 5) 送信
    ctx.say(msg).await?;
    Ok(())
}
