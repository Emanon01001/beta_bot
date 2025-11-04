use anyhow::{anyhow, Context as AnyhowContext};
use poise::CreateReply;
use serde::{Deserialize, Serialize};

use crate::{
    get_http_client, GLOBAL_CONFIG,
    util::alias::{Context as PoiseContext, Error},
};

const CHAT_COMPLETIONS_URL: &str = "https://nano-gpt.com/api/v1/chat/completions";
const MAX_DISCORD_MESSAGE: usize = 1900;

#[poise::command(slash_command, prefix_command)]
pub async fn chat(ctx: PoiseContext<'_>, #[rest] prompt: String) -> Result<(), Error> {
    if prompt.trim().is_empty() {
        ctx.say("❌ プロンプトが空です").await?;
        return Ok(());
    }

    let status = ctx
        .send(CreateReply::default().content("⌛ 待機中…"))
        .await?;

    match request_chat_completion(&prompt).await {
        Ok(content) => {
            let reply = if content.chars().count() > MAX_DISCORD_MESSAGE {
                let mut truncated = content.chars().take(MAX_DISCORD_MESSAGE).collect::<String>();
                truncated.push_str("...(truncated)");
                format!("```{truncated}```")
            } else {
                content
            };
            status
                .edit(ctx, CreateReply::default().content(reply))
                .await?;
        }
        Err(err) => {
            status
                .edit(
                    ctx,
                    CreateReply::default()
                        .content(format!("❌ API リクエストに失敗しました: {err}")),
                )
                .await?;
        }
    }

    Ok(())
}

async fn request_chat_completion(prompt: &str) -> anyhow::Result<String> {
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        return Err(anyhow!("prompt must not be empty"));
    }

    let api_key = GLOBAL_CONFIG.token.api_key.trim();
    if api_key.is_empty() {
        return Err(anyhow!("nano-gpt API key is missing in the configuration"));
    }

    let body = ChatRequest {
        model: "huihui-ai/Llama-3.3-70B-Instruct-abliterated",
        messages: vec![MessagePayload {
            role: "user",
            content: trimmed_prompt,
        }],
    };

    let client = get_http_client();
    let response = client
        .post(CHAT_COMPLETIONS_URL)
        .bearer_auth(api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .with_context(|| "failed to send chat completion request")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read error body>".into());
        return Err(anyhow!(
            "HTTP error: {} {}\n{}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
            error_body
        ));
    }

    let payload: ChatResponse = response
        .json()
        .await
        .with_context(|| "failed to parse chat completion response")?;

    let content = payload
        .choices
        .into_iter()
        .find_map(|choice| choice.message.and_then(|m| m.content))
        .ok_or_else(|| anyhow!("response did not contain a message content"))?;

    Ok(content)
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<MessagePayload<'a>>,
}

#[derive(Serialize)]
struct MessagePayload<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<ChoiceMessage>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}
