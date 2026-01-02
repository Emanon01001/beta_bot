use poise::serenity_prelude::{ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed};
use serde_json::json;
use std::time::Duration;

use crate::util::alias::{Context, Error};

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
pub async fn pages(ctx: Context<'_>) -> Result<(), Error> {
    // ページ分割のサンプル
    let pages = &[
        "**ページ 1**\nこれはページ 1 の内容です",
        "**ページ 2**\nこれはページ 2 の内容です",
        "**ページ 3**\nこれはページ 3 の内容です",
    ];

    poise::builtins::paginate(ctx, pages).await?;

    Ok(())
}
