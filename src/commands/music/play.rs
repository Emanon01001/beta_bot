use crate::{
    commands::music::join::_join,
    util::{
        alias::Context,
        play::play_track_req,
        queue::MusicQueue,
        track::TrackRequest,
        types::PlayingMap,
    },
    Error,
};
use chrono::Utc;
use dashmap::DashMap;
use poise::serenity_prelude::{
    ButtonStyle, Colour, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse, GuildId,
    Message,
};
use poise::CreateReply;
use songbird::{tracks::PlayMode, Call};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const ACCENT: Colour = Colour::new(0x5865F2);
const SUCCESS: Colour = Colour::new(0x2ECC71);
const WARNING: Colour = Colour::new(0xF1C40F);
const DANGER: Colour = Colour::new(0xE74C3C);
const CONTROL_WINDOW: Duration = Duration::from_secs(180);

/// 秒数を mm:ss 形式に整形する（不明なら "--:--"）。
fn format_duration(dur: Option<Duration>) -> String {
    dur.map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
        .unwrap_or_else(|| "--:--".to_string())
}

/// 曲情報を Embed に整形する（タイトル/リンク/長さ/リクエスト者）。
fn track_embed(
    title: &str,
    tr: Option<&TrackRequest>,
    note: Option<String>,
    colour: Colour,
) -> CreateEmbed {
    let mut embed = CreateEmbed::default()
        .title(title)
        .colour(colour)
        .timestamp(Utc::now());

    if let Some(note) = note {
        embed = embed.description(note);
    }

    if let Some(tr) = tr {
        let title = tr.meta.title.as_deref().unwrap_or(&tr.url);
        let link = tr.meta.source_url.as_deref().unwrap_or(&tr.url);
        embed = embed.field("Track", format!("[{}]({})", title, link), false);
        embed = embed.field("Length", format_duration(tr.meta.duration), true);
        embed = embed.field("Requested by", format!("<@{}>", tr.requested_by), true);
    }

    embed
}

/// 再生ステートに合わせてボタン行を生成する。
fn control_components(state: PlayMode) -> Vec<CreateActionRow> {
    let is_playing = matches!(state, PlayMode::Play);
    let is_paused = matches!(state, PlayMode::Pause);
    vec![CreateActionRow::Buttons(vec![
        CreateButton::new("music_pause")
            .label("⏸ 一時停止")
            .style(ButtonStyle::Secondary)
            .disabled(!is_playing),
        CreateButton::new("music_resume")
            .label("▶ 再開")
            .style(ButtonStyle::Secondary)
            .disabled(!is_paused),
        CreateButton::new("music_skip")
            .label("⏭ 次の曲へ")
            .style(ButtonStyle::Primary),
        CreateButton::new("music_stop")
            .label("⏹ 停止")
        .style(ButtonStyle::Danger),
    ])]
}

/// 再生を止め、キューと状態をリセットする。
async fn stop_playback(ctx: &Context<'_>, gid: GuildId) -> Result<(), Error> {
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird 未初期化")?;
    let call = manager
        .get(gid)
        .ok_or("VC に接続していません")?
        .clone();

    call.lock().await.stop();
    ctx.data().queues.remove(&gid);
    ctx.data().playing.remove(&gid);
    Ok(())
}

/// Embed + ボタン付きのコントロールメッセージを送信する。
async fn send_control_message(
    ctx: &Context<'_>,
    embed: CreateEmbed,
    controls: PlayMode,
) -> Result<Message, Error> {
    let reply = CreateReply::default()
        .embed(embed)
        .components(control_components(controls));
    let handle = ctx.send(reply).await?;
    Ok(handle.message().await?.into_owned())
}

/// 既存メッセージを Update として書き換える。
async fn update_message(
    ctx: &Context<'_>,
    interaction: &ComponentInteraction,
    embed: CreateEmbed,
    components: Vec<CreateActionRow>,
) {
    let builder = CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::default()
            .embeds(vec![embed])
            .components(components),
    );
    let _ = interaction.create_response(ctx.serenity_context(), builder).await;
}

/// ボタン押下に対し、エフェメラルで短い応答を返す。
async fn respond_ephemeral(
    ctx: &Context<'_>,
    interaction: &ComponentInteraction,
    content: &str,
) {
    let builder = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::default()
            .content(content)
            .ephemeral(true),
    );
    let _ = interaction.create_response(ctx.serenity_context(), builder).await;
}

/// ボタン（停止/一時停止/再開/次へ）を処理し、メッセージを更新する。
async fn handle_controls(
    ctx: &Context<'_>,
    gid: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: PlayingMap,
    msg: Message,
) -> Result<(), Error> {
    let start = Instant::now();
    loop {
        if start.elapsed() >= CONTROL_WINDOW {
            break;
        }
        let timeout = CONTROL_WINDOW - start.elapsed();
        let Some(interaction) = msg
            .await_component_interaction(ctx)
            .author_id(ctx.author().id)
            .timeout(timeout)
            .await
        else {
            break;
        };

        let custom = interaction.data.custom_id.as_str();
        match custom {
            "music_stop" => {
                stop_playback(ctx, gid).await?;
                let embed = track_embed(
                    "⏹ 再生を停止しました",
                    None,
                    Some("キューをクリアしました。".into()),
                    ACCENT,
                );
                update_message(&ctx, &interaction, embed, Vec::new()).await;
                break;
            }
            "music_pause" => {
                if let Some(entry) = playing.get(&gid) {
                    let (handle, req) = entry.value();
                    if matches!(handle.get_info().await.map(|i| i.playing), Ok(PlayMode::Play)) {
                        let _ = handle.pause();
                        let embed = track_embed("⏸ 一時停止しました", Some(req), None, ACCENT);
                        update_message(&ctx, &interaction, embed, control_components(PlayMode::Pause))
                            .await;
                        continue;
                    } else {
                        respond_ephemeral(&ctx, &interaction, "⏸ すでに一時停止中です").await;
                    }
                } else {
                    respond_ephemeral(&ctx, &interaction, "再生中の曲がありません").await;
                }
            }
            "music_resume" => {
                if let Some(entry) = playing.get(&gid) {
                    let (handle, req) = entry.value();
                    if matches!(handle.get_info().await.map(|i| i.playing), Ok(PlayMode::Pause)) {
                        let _ = handle.play();
                        let embed =
                            track_embed("▶ 再生を再開しました", Some(req), None, SUCCESS);
                        update_message(&ctx, &interaction, embed, control_components(PlayMode::Play))
                            .await;
                        continue;
                    } else {
                        respond_ephemeral(&ctx, &interaction, "再生を再開できませんでした").await;
                    }
                } else {
                    respond_ephemeral(&ctx, &interaction, "再生中の曲がありません").await;
                }
            }
            "music_skip" => {
                // すぐ応答を返して「Interaction failed」を防ぐ
                let _ = interaction
                    .create_response(
                        ctx.serenity_context(),
                        CreateInteractionResponse::Acknowledge,
                    )
                    .await;

                if let Some(entry) = playing.get(&gid) {
                    let (handle, _) = entry.value();
                    let _ = handle.stop();
                }

                let next_req = if let Some(mut q) = queues.get_mut(&gid) {
                    let remaining_after = q.len().saturating_sub(1);
                    q.pop_next().map(|req| (req, remaining_after))
                } else {
                    None
                };

                if let Some((next_req, remaining_after)) = next_req {
                    match play_track_req(
                        gid,
                        call.clone(),
                        queues.clone(),
                        playing.clone(),
                        next_req,
                    )
                    .await
                    {
                        Ok((_handle, started_req)) => {
                            let embed = track_embed(
                                "⏭ 次の曲を再生しました",
                                Some(&started_req),
                                Some(format!("キュー残り {} 件", remaining_after)),
                                SUCCESS,
                            );
                            let _ = interaction
                                .edit_response(
                                    ctx.serenity_context(),
                                    EditInteractionResponse::new()
                                        .embeds(vec![embed.clone()])
                                        .components(control_components(PlayMode::Play)),
                                )
                                .await;
                            continue;
                        }
                        Err(e) => {
                            let embed = track_embed(
                                "⚠️ 次曲の再生に失敗しました",
                                None,
                                Some(format!("{e}")),
                                DANGER,
                            );
                            let _ = interaction
                                .edit_response(
                                    ctx.serenity_context(),
                                    EditInteractionResponse::new()
                                        .embeds(vec![embed.clone()])
                                        .components(Vec::new()),
                                )
                                .await;
                            break;
                        }
                    }
                } else {
                    let embed = track_embed(
                        "🎶 キューが空です",
                        None,
                        Some("次の曲がないため、再生を停止しました。".into()),
                        WARNING,
                    );
                    let _ = interaction
                        .edit_response(
                            ctx.serenity_context(),
                            EditInteractionResponse::new()
                                .embeds(vec![embed.clone()])
                                .components(Vec::new()),
                        )
                        .await;
                }
            }
            _ => {
                respond_ephemeral(&ctx, &interaction, "不明な操作です").await;
            }
        }
    }
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[rest]
    #[description = "YouTube URL または検索語 (空で再開)"]
    query: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    _join(&ctx, gid, None).await?;
    let call = songbird::get(ctx.serenity_context())
        .await
        .and_then(|m| m.get(gid))
        .ok_or("VC に接続していません")?
        .clone();

    let queues = ctx.data().queues.clone();
    let playing = ctx.data().playing.clone();
    let author = ctx.author().id;

    let (current_handle, current_state, current_req) = if let Some(entry) = playing.get(&gid) {
        let (handle, req) = entry.value();
        let state = handle
            .get_info()
            .await
            .map(|info| info.playing)
            .unwrap_or(PlayMode::Stop);
        (Some(handle.clone()), state, Some(req.clone()))
    } else {
        (None, PlayMode::Stop, None)
    };

    if query.is_none() && current_state == PlayMode::Pause {
        if let Some(h) = current_handle {
            let _ = h.play();
            let embed = track_embed(
                "▶ 再生を再開しました",
                current_req.as_ref(),
                Some("一時停止中のトラックを続きから再生します。".into()),
                SUCCESS,
            );
            let msg = send_control_message(&ctx, embed, PlayMode::Play).await?;
            handle_controls(
                &ctx,
                gid,
                call.clone(),
                queues.clone(),
                playing.clone(),
                msg,
            )
            .await?;
            return Ok(());
        }
    }

    if let Some(q) = query {
        match TrackRequest::from_url(q, author).await {
            Ok(req) => {
                if current_state == PlayMode::Play {
                    let position = {
                        let mut guard = queues.entry(gid).or_default();
                        let pos = guard.len() + 1;
                        guard.push_back(req.clone());
                        pos
                    };
                    let embed = track_embed(
                        "📥 キューに追加しました",
                        Some(&req),
                        Some(format!("現在再生中です。キュー #{position} に追加しました。")),
                        ACCENT,
                    );
                    let msg =
                        send_control_message(&ctx, embed, current_state).await?;
                    handle_controls(
                        &ctx,
                        gid,
                        call.clone(),
                        queues.clone(),
                        playing.clone(),
                        msg,
                    )
                    .await?;
                    return Ok(());
                } else {
                    match play_track_req(
                        gid,
                        call.clone(),
                        queues.clone(),
                        playing.clone(),
                        req,
                    )
                    .await
                    {
                        Ok((_handle, next_req)) => {
                            let embed = track_embed(
                                "🎵 再生を開始しました",
                                Some(&next_req),
                                Some("このトラックから再生を始めます。".into()),
                                SUCCESS,
                            );
                            let msg =
                                send_control_message(&ctx, embed, PlayMode::Play).await?;
                            handle_controls(
                                &ctx,
                                gid,
                                call.clone(),
                                queues.clone(),
                                playing.clone(),
                                msg,
                            )
                            .await?;
                            return Ok(());
                        }
                        Err(e) => {
                            let embed = track_embed(
                                "⚠️ 再生開始に失敗しました",
                                None,
                                Some(format!("{e}")),
                                DANGER,
                            );
                            let _ = ctx
                                .send(CreateReply::default().embed(embed))
                                .await;
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                let embed = track_embed(
                    "⚠️ リクエスト生成に失敗しました",
                    None,
                    Some(e.to_string()),
                    DANGER,
                );
                let _ = ctx.send(CreateReply::default().embed(embed)).await;
                return Ok(());
            }
        }
    }

    if current_state != PlayMode::Play {
        playing.remove(&gid);

        let next_req = if let Some(mut q) = queues.get_mut(&gid) {
            let remaining_after = q.len().saturating_sub(1);
            q.pop_next().map(|req| (req, remaining_after))
        } else {
            None
        };

        if let Some((next_req, remaining_after)) = next_req {
            match play_track_req(
                gid,
                call.clone(),
                queues.clone(),
                playing.clone(),
                next_req,
            )
            .await
            {
                Ok((_handle, started_req)) => {
                    let embed = track_embed(
                        "⏭ 次の曲を再生しました",
                        Some(&started_req),
                        Some(format!("キュー残り {} 件", remaining_after)),
                        SUCCESS,
                    );
                    let msg =
                        send_control_message(&ctx, embed, PlayMode::Play).await?;
                    handle_controls(
                        &ctx,
                        gid,
                        call.clone(),
                        queues.clone(),
                        playing.clone(),
                        msg,
                    )
                    .await?;
                    return Ok(());
                }
                Err(e) => {
                    let embed = track_embed(
                        "⚠️ 次曲の再生に失敗しました",
                        None,
                        Some(format!("{e}")),
                        DANGER,
                    );
                    let _ = ctx.send(CreateReply::default().embed(embed)).await;
                    return Ok(());
                }
            }
        } else {
            let embed = track_embed(
                "🎶 キューに曲がありません",
                None,
                Some("追加する曲を指定してください。".into()),
                WARNING,
            );
            let _ = ctx.send(CreateReply::default().embed(embed)).await;
            return Ok(());
        }
    }

    let embed = track_embed(
        "🎧 既に再生中です",
        current_req.as_ref(),
        Some("新しい曲を再生するにはクエリを指定してください。".into()),
        ACCENT,
    );
    let msg = send_control_message(&ctx, embed, current_state).await?;
    handle_controls(
        &ctx,
        gid,
        call.clone(),
        queues.clone(),
        playing.clone(),
        msg,
    )
    .await?;
    Ok(())
}
