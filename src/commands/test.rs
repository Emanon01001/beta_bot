use poise::serenity_prelude::{ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed};
use serde_json::json;
use std::time::Duration;

use crate::{
    get_http_client, util::alias::{Context, Error}, GLOBAL_CONFIG
};

#[poise::command(prefix_command)]
pub async fn button_test(ctx: Context<'_>) -> Result<(), Error> {
    // アクション行（ボタン）を作成
    let action_row = CreateActionRow::Buttons(vec![
        CreateButton::new("button_1")
            .custom_id("button_1")
            .label("ボタン 1")
            .style(ButtonStyle::Primary),
    ]);

    // 埋め込みを作成
    let embed = CreateEmbed::new()
        .title("ボタンテスト")
        .description("以下のボタンをクリックしてください")
        .color(Colour::DARK_BLUE);

    // ボタン付きのメッセージを送信
    let sent_msg = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .components(vec![action_row]),
        )
        .await?;

    // 60 秒間、指定されたボタンのクリックを待ち受ける
    // 送信したメッセージIDを使って、コンポーネントインタラクションを待機する例
    let component_interaction = sent_msg.message().await?.await_component_interaction(ctx);

    let interaction = component_interaction
        .timeout(Duration::from_secs(60))
        .await
        .unwrap();

    let _interaction = interaction.data.custom_id == "button_1";
    {
        if interaction.data.custom_id == "button_1" {
            let response_json = json!({
                "type": 6,
            });

            ctx.http()
                .create_interaction_response(
                    interaction.id,
                    &interaction.token,
                    &response_json,
                    vec![],
                )
                .await?;
            // ボタンがクリックされたので、応答メッセージを送信
            ctx.send(poise::CreateReply::default().content("ボタンがクリックされました"))
                .await?;
        } else {
            ctx.send(poise::CreateReply::default().content("タイムアウトしました"))
                .await?;
        }
    }

    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn exec(ctx: Context<'_>, #[rest] prompt: String) -> Result<(), Error> {
    if prompt.trim().is_empty() {
        ctx.say("❌ プロンプトが空です").await?;
        return Ok(());
    }

    ctx.defer().await?; // 処理中の表示
    // Gemini APIリクエスト本体
    let client = get_http_client();
    let url =
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";

    // リクエストボディ
    let body = json!({
        "system_instruction" : {
            "parts" : [
                { "text": "※Markdown形式を絶対に使わず、必ずプレーンテキストで回答してください。" }
            ]
        },
        "contents": [
            {
                "parts": [
                    { "text": prompt}
                ]
            }
        ]
    });

    // リクエスト送信
    let res = client
        .post(url)
        .header("x-goog-api-key", &GLOBAL_CONFIG.token.api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::from(format!("リクエスト失敗: {e}")))?;

    let status = res.status();
    let text = res.text().await.unwrap_or_default();

    // レスポンスから生成テキストを抽出
    let response_json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();

    // "candidates"[0]."content"."parts"[0]."text" を抽出（APIによってパス要調整）
    let mut answer = response_json["candidates"]
        .get(0)
        .and_then(|c| c["content"]["parts"].get(0))
        .and_then(|p| p["text"].as_str())
        .unwrap_or("❌ レスポンス解析失敗…")
        .to_string();

    let max_chars = 1900;

    if answer.chars().count() > max_chars {
        let truncated: String = answer.chars().take(max_chars).collect();
        answer = format!("```{truncated}...(truncated)```");
    }

    // Discordに出力
    let reply = format!("**Status:** `{}`\n**Result:**\n```{}```", status, answer);
    ctx.say(reply).await?;

    Ok(())
}
