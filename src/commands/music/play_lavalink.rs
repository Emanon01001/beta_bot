use crate::{
    Error,
    commands::music::join::_join,
    util::{
        alias::Context,
        lavalink_player::{
            current_play_mode, pause_current_lavalink, play_next_from_queue_lavalink,
            play_track_req_lavalink, resume_current_lavalink, stop_and_clear_lavalink,
        },
        music_ui::{control_components, track_embed},
        player::{ManualTransitionGuard, PlaybackControlResult},
        playlist,
        queue::MusicQueue,
        track::TrackRequest,
        types::LavalinkPlayingMap,
    },
};
use dashmap::DashMap;
use lavalink_rs::client::LavalinkClient;
use poise::CreateReply;
use poise::builtins::paginate;
use poise::serenity_prelude::{
    Colour, ComponentInteraction, CreateActionRow, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditMessage, GuildId, Message,
};
use songbird::tracks::PlayMode;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const ACCENT: Colour = Colour::new(0x5865F2);
const SUCCESS: Colour = Colour::new(0x2ECC71);
const WARNING: Colour = Colour::new(0xF1C40F);
const DANGER: Colour = Colour::new(0xE74C3C);
const CONTROL_IDLE_TIMEOUT: Duration = Duration::from_secs(1800);
const MAX_PLAYLIST_ITEMS: usize = 50;

fn playlist_pages(urls: &[String], title: &str) -> Vec<String> {
    const PAGE_SIZE: usize = 10;
    urls.chunks(PAGE_SIZE)
        .enumerate()
        .map(|(pi, chunk)| {
            let mut s = format!(
                "📃 {title} ({}/{})\n\n",
                pi + 1,
                (urls.len() + PAGE_SIZE - 1) / PAGE_SIZE
            );
            for (i, url) in chunk.iter().enumerate() {
                let idx = pi * PAGE_SIZE + i + 1;
                s.push_str(&format!("{idx}. {url}\n"));
            }
            s
        })
        .collect()
}

async fn send_control_message(
    ctx: &Context<'_>,
    gid: GuildId,
    embed: CreateEmbed,
    controls: PlayMode,
) -> Result<Message, Error> {
    let reply = CreateReply::default()
        .embed(embed)
        .components(control_components(controls));
    let handle = ctx.send(reply).await?;
    let msg = handle.message().await?.into_owned();
    ctx.data().now_playing.insert(gid, (msg.channel_id, msg.id));
    Ok(msg)
}

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
    let _ = interaction
        .create_response(ctx.serenity_context(), builder)
        .await;
}

async fn respond_ephemeral(ctx: &Context<'_>, interaction: &ComponentInteraction, content: &str) {
    let builder = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::default()
            .content(content)
            .ephemeral(true),
    );
    let _ = interaction
        .create_response(ctx.serenity_context(), builder)
        .await;
}

async fn handle_controls(
    ctx: &Context<'_>,
    gid: GuildId,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: LavalinkPlayingMap,
    lavalink: Arc<LavalinkClient>,
    mut msg: Message,
) -> Result<(), Error> {
    let mut deadline = Instant::now() + CONTROL_IDLE_TIMEOUT;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }

        let timeout = deadline.saturating_duration_since(now);
        let Some(interaction) = msg.await_component_interaction(ctx).timeout(timeout).await else {
            break;
        };

        if interaction.user.id != ctx.author().id {
            respond_ephemeral(&ctx, &interaction, "この操作はコマンド実行者のみ可能です").await;
            continue;
        }

        deadline = Instant::now() + CONTROL_IDLE_TIMEOUT;

        match interaction.data.custom_id.as_str() {
            "music_stop" => {
                let _guard = ManualTransitionGuard::acquire(&ctx.data().transition_flags, gid);
                stop_and_clear_lavalink(ctx, gid).await?;
                let embed = track_embed(
                    "⏹ 再生を停止しました",
                    None,
                    Some("キューをクリアしました。".into()),
                    ACCENT,
                );
                update_message(&ctx, &interaction, embed, Vec::new()).await;
                break;
            }
            "music_pause" => match pause_current_lavalink(&lavalink, gid, &playing).await? {
                PlaybackControlResult::Changed(req) => {
                    let embed = track_embed("⏸ 一時停止しました", Some(&req), None, ACCENT);
                    update_message(
                        &ctx,
                        &interaction,
                        embed,
                        control_components(PlayMode::Pause),
                    )
                    .await;
                    continue;
                }
                PlaybackControlResult::Unchanged => {
                    respond_ephemeral(&ctx, &interaction, "⏸ すでに一時停止中です").await;
                }
                PlaybackControlResult::Missing => {
                    respond_ephemeral(&ctx, &interaction, "再生中の曲がありません").await;
                }
            },
            "music_resume" => match resume_current_lavalink(&lavalink, gid, &playing).await? {
                PlaybackControlResult::Changed(req) => {
                    let embed = track_embed("▶ 再生を再開しました", Some(&req), None, SUCCESS);
                    update_message(
                        &ctx,
                        &interaction,
                        embed,
                        control_components(PlayMode::Play),
                    )
                    .await;
                    continue;
                }
                PlaybackControlResult::Unchanged => {
                    respond_ephemeral(&ctx, &interaction, "再生を再開できませんでした").await;
                }
                PlaybackControlResult::Missing => {
                    respond_ephemeral(&ctx, &interaction, "再生中の曲がありません").await;
                }
            },
            "music_skip" => {
                let embed = track_embed("⏳ 次の曲を準備しています…", None, None, ACCENT);
                update_message(&ctx, &interaction, embed, Vec::new()).await;

                let _guard = ManualTransitionGuard::acquire(&ctx.data().transition_flags, gid);
                if let Some(player) = lavalink.get_player_context(gid) {
                    let _ = player.stop_now().await;
                }
                playing.remove(&gid);

                let res = play_next_from_queue_lavalink(
                    gid,
                    lavalink.clone(),
                    queues.clone(),
                    playing.clone(),
                    ctx.data().history.clone(),
                    3,
                )
                .await?;

                if let Some(started_req) = res.started {
                    let info = if res.skipped > 0 {
                        format!(
                            "再生失敗 {} 件をスキップ / キュー残り {} 件",
                            res.skipped, res.remaining
                        )
                    } else {
                        format!("キュー残り {} 件", res.remaining)
                    };
                    let embed = track_embed(
                        "⏭ 次の曲を再生しました",
                        Some(&started_req),
                        Some(info),
                        SUCCESS,
                    );
                    let _ = msg
                        .edit(
                            ctx.serenity_context(),
                            EditMessage::new()
                                .embeds(vec![embed])
                                .components(control_components(PlayMode::Play)),
                        )
                        .await;
                    continue;
                }

                let detail = res
                    .last_error
                    .or_else(|| Some(format!("次の曲がありません (残り {} 件)", res.remaining)));
                let embed = track_embed("⚠️ 次曲の再生に失敗しました", None, detail, DANGER);
                let _ = msg
                    .edit(
                        ctx.serenity_context(),
                        EditMessage::new()
                            .embeds(vec![embed])
                            .components(Vec::new()),
                    )
                    .await;
                break;
            }
            _ => {
                respond_ephemeral(&ctx, &interaction, "不明な操作です").await;
            }
        }
    }

    let _ = msg
        .edit(
            ctx.serenity_context(),
            EditMessage::new().components(Vec::new()),
        )
        .await;
    Ok(())
}

pub async fn run(ctx: &Context<'_>, gid: GuildId, query: Option<String>) -> Result<(), Error> {
    let lavalink = ctx
        .data()
        .lavalink
        .clone()
        .ok_or("Lavalink client is not initialized")?;

    _join(ctx, gid, None).await?;

    let queues = ctx.data().queues.clone();
    let playing = ctx.data().lavalink_playing.clone();
    let author = ctx.author().id;
    let current_state = current_play_mode(&lavalink, gid).await;
    let current_req = playing.get(&gid).map(|e| e.value().clone());

    if query.is_none() && current_state == PlayMode::Pause {
        match resume_current_lavalink(&lavalink, gid, &playing).await? {
            PlaybackControlResult::Changed(req) => {
                let embed = track_embed(
                    "▶ 再生を再開しました",
                    Some(&req),
                    Some("一時停止中のトラックを続きから再生します。".into()),
                    SUCCESS,
                );
                let msg = send_control_message(ctx, gid, embed, PlayMode::Play).await?;
                handle_controls(
                    ctx,
                    gid,
                    queues.clone(),
                    playing.clone(),
                    lavalink.clone(),
                    msg,
                )
                .await?;
                return Ok(());
            }
            PlaybackControlResult::Unchanged => {}
            PlaybackControlResult::Missing => {}
        }
    }

    if let Some(q) = query {
        if playlist::is_youtube_playlist_url(&q) {
            match playlist::expand_youtube_playlist(&q, MAX_PLAYLIST_ITEMS).await {
                Ok(urls) => {
                    let pages = playlist_pages(&urls, "プレイリスト展開結果");
                    let page_slices: Vec<&str> = pages.iter().map(String::as_str).collect();

                    let mut reqs = urls
                        .into_iter()
                        .map(|u| TrackRequest::new(u, author))
                        .collect::<Vec<_>>();
                    let total = reqs.len();
                    let preview = reqs
                        .first()
                        .cloned()
                        .ok_or_else(|| Error::from("プレイリストが空でした"))?;

                    if current_state == PlayMode::Play {
                        let (position_start, position_end) = {
                            let mut guard = queues.entry(gid).or_default();
                            let start = guard.len() + 1;
                            for r in reqs {
                                guard.push_back(r);
                            }
                            let end = start + total.saturating_sub(1);
                            (start, end)
                        };

                        let embed = track_embed(
                            "📃 プレイリストをキューに追加しました",
                            Some(&preview),
                            Some(format!(
                                "{total} 件を展開しました。キュー #{position_start}〜#{position_end} に追加しました。"
                            )),
                            ACCENT,
                        );
                        let msg = send_control_message(ctx, gid, embed, current_state).await?;
                        handle_controls(
                            ctx,
                            gid,
                            queues.clone(),
                            playing.clone(),
                            lavalink.clone(),
                            msg,
                        )
                        .await?;
                        paginate(*ctx, &page_slices).await?;
                        return Ok(());
                    } else {
                        let first = reqs.remove(0);
                        {
                            let mut guard = queues.entry(gid).or_default();
                            for r in reqs {
                                guard.push_back(r);
                            }
                        }

                        match play_track_req_lavalink(
                            gid,
                            lavalink.clone(),
                            playing.clone(),
                            ctx.data().history.clone(),
                            first,
                        )
                        .await
                        {
                            Ok(started_req) => {
                                let remaining = queues.get(&gid).map(|q| q.len()).unwrap_or(0);
                                let embed = track_embed(
                                    "🎶 再生を開始しました",
                                    Some(&started_req),
                                    Some(format!(
                                        "プレイリスト {total} 件を展開しました。キュー残り {remaining} 件"
                                    )),
                                    SUCCESS,
                                );
                                let msg =
                                    send_control_message(ctx, gid, embed, PlayMode::Play).await?;
                                handle_controls(
                                    ctx,
                                    gid,
                                    queues.clone(),
                                    playing.clone(),
                                    lavalink.clone(),
                                    msg,
                                )
                                .await?;
                                paginate(*ctx, &page_slices).await?;
                                return Ok(());
                            }
                            Err(e) => {
                                let embed = track_embed(
                                    "❌ 再生開始に失敗しました",
                                    None,
                                    Some(format!("{e}")),
                                    DANGER,
                                );
                                let _ = ctx.send(CreateReply::default().embed(embed)).await;
                                return Ok(());
                            }
                        }
                    }
                }
                Err(e) => {
                    let embed = track_embed(
                        "❌ プレイリスト展開に失敗しました",
                        None,
                        Some(e.to_string()),
                        DANGER,
                    );
                    let _ = ctx.send(CreateReply::default().embed(embed)).await;
                    return Ok(());
                }
            }
        }

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
                        Some(format!(
                            "現在再生中です。キュー #{position} に追加しました。"
                        )),
                        ACCENT,
                    );
                    let msg = send_control_message(ctx, gid, embed, current_state).await?;
                    handle_controls(
                        ctx,
                        gid,
                        queues.clone(),
                        playing.clone(),
                        lavalink.clone(),
                        msg,
                    )
                    .await?;
                    return Ok(());
                } else {
                    match play_track_req_lavalink(
                        gid,
                        lavalink.clone(),
                        playing.clone(),
                        ctx.data().history.clone(),
                        req,
                    )
                    .await
                    {
                        Ok(next_req) => {
                            let embed = track_embed(
                                "🎵 再生を開始しました",
                                Some(&next_req),
                                Some("このトラックから再生を始めます。".into()),
                                SUCCESS,
                            );
                            let msg = send_control_message(ctx, gid, embed, PlayMode::Play).await?;
                            handle_controls(
                                ctx,
                                gid,
                                queues.clone(),
                                playing.clone(),
                                lavalink.clone(),
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
                            let _ = ctx.send(CreateReply::default().embed(embed)).await;
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
        let res = play_next_from_queue_lavalink(
            gid,
            lavalink.clone(),
            queues.clone(),
            playing.clone(),
            ctx.data().history.clone(),
            3,
        )
        .await?;

        if let Some(started_req) = res.started {
            let info = if res.skipped > 0 {
                format!(
                    "再生失敗 {} 件をスキップ / キュー残り {} 件",
                    res.skipped, res.remaining
                )
            } else {
                format!("キュー残り {} 件", res.remaining)
            };
            let embed = track_embed(
                "⏭ 次の曲を再生しました",
                Some(&started_req),
                Some(info),
                SUCCESS,
            );
            let msg = send_control_message(ctx, gid, embed, PlayMode::Play).await?;
            handle_controls(
                ctx,
                gid,
                queues.clone(),
                playing.clone(),
                lavalink.clone(),
                msg,
            )
            .await?;
            return Ok(());
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
    let msg = send_control_message(ctx, gid, embed, current_state).await?;
    handle_controls(
        ctx,
        gid,
        queues.clone(),
        playing.clone(),
        lavalink.clone(),
        msg,
    )
    .await?;
    Ok(())
}
