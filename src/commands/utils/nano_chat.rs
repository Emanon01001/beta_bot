use anyhow::{Context as AnyhowContext, anyhow};
use poise::CreateReply;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::time::{self, MissedTickBehavior};

use crate::{
    GLOBAL_CONFIG, get_http_client,
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

    let started_at = Instant::now();

    let mut interval = time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval.tick().await;

    let mut last_reported_secs = 0u64;
    let mut can_update_status = true;
    let mut request_fut = Box::pin(request_chat_completion(&prompt));

    let result = loop {
        tokio::select! {
            res = &mut request_fut => break res,
            _ = interval.tick() => {
                if !can_update_status {
                    continue;
                }

                let elapsed = started_at.elapsed();
                let secs = elapsed.as_secs();
                if secs == 0 || secs == last_reported_secs {
                    continue;
                }
                last_reported_secs = secs;

                let wait_text = format_elapsed(elapsed);
                let content = format!("⌛ 待機中… ({wait_text})");
                if status.edit(ctx, CreateReply::default().content(content)).await.is_err() {
                    can_update_status = false;
                }
            }
        }
    };

    let waited_text = format_elapsed(started_at.elapsed());

    match result {
        Ok(content) => {
            let reply = if content.chars().count() > MAX_DISCORD_MESSAGE {
                let mut truncated = content
                    .chars()
                    .take(MAX_DISCORD_MESSAGE)
                    .collect::<String>();
                truncated.push_str("...(truncated)");
                format!("```{truncated}```\n\n(待機: {waited_text})")
            } else {
                format!("{content}\n\n(待機: {waited_text})")
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
                        .content(format!(
                            "❌ API リクエストに失敗しました (待機: {waited_text}): {err}"
                        )),
                )
                .await?;
        }
    }

    Ok(())
}

fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        if secs == 0 {
            return format!("{:.1}秒", elapsed.as_secs_f32());
        }
        return format!("{secs}秒");
    }

    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes}分{seconds:02}秒")
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
