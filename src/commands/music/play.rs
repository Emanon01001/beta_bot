use crate::{
    Error,
    commands::music::join::_join,
    util::{
        alias::Context,
        play::{play_next_from_queue, play_track_req},
        playlist,
        queue::MusicQueue,
        track::TrackRequest,
        types::{PlayingMap, TransitionFlags},
    },
};
use chrono::Utc;
use dashmap::DashMap;
use poise::CreateReply;
use poise::builtins::paginate;
use poise::serenity_prelude::{
    ButtonStyle, Colour, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage, GuildId, Message,
};
use songbird::{Call, tracks::PlayMode};
use std::{
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use url::Url;

const ACCENT: Colour = Colour::new(0x5865F2);
const SUCCESS: Colour = Colour::new(0x2ECC71);
const WARNING: Colour = Colour::new(0xF1C40F);
const DANGER: Colour = Colour::new(0xE74C3C);
const CONTROL_IDLE_TIMEOUT: Duration = Duration::from_secs(1800);
const MAX_PLAYLIST_ITEMS: usize = 50;

fn transition_flag(flags: &TransitionFlags, gid: GuildId) -> Arc<AtomicBool> {
    flags
        .entry(gid)
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let len = s.chars().count();
    if len <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out = s.chars().take(keep).collect::<String>();
    out.push('…');
    out
}

fn truncate_embed_title(s: &str) -> String {
    truncate_chars(s, 256)
}

fn truncate_embed_description(s: &str) -> String {
    truncate_chars(s, 4096)
}

fn truncate_embed_field_value(s: &str) -> String {
    truncate_chars(s, 1024)
}

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

/// 秒数を mm:ss 形式に整形する（不明なら "--:--"）。
fn format_duration(dur: Option<Duration>) -> String {
    dur.map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
        .unwrap_or_else(|| "--:--".to_string())
}

/// YouTube の URL からサムネイル URL を導出する。
fn youtube_thumbnail(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str().unwrap_or_default();
    if host.contains("youtube.com") {
        if let Some(id) = parsed
            .query_pairs()
            .find_map(|(k, v)| (k == "v").then_some(v))
        {
            return Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"));
        }
    }
    if host.contains("youtu.be") || host.contains("m.youtube.com") {
        if let Some(seg) = parsed.path_segments().and_then(|mut s| s.next()) {
            if !seg.is_empty() {
                return Some(format!("https://i.ytimg.com/vi/{seg}/hqdefault.jpg"));
            }
        }
    }
    None
}

/// 曲情報を Embed に整形する（タイトル/リンク/長さ/リクエスト者/サムネイル）。
pub(crate) fn track_embed(
    title: &str,
    tr: Option<&TrackRequest>,
    note: Option<String>,
    colour: Colour,
) -> CreateEmbed {
    let mut embed = CreateEmbed::default()
        .title(truncate_embed_title(title))
        .colour(colour)
        .timestamp(Utc::now());

    if let Some(note) = note {
        embed = embed.description(truncate_embed_description(&note));
    }

    if let Some(tr) = tr {
        let track_title = tr.meta.title.as_deref().unwrap_or(&tr.url);
        let track_link = tr.meta.source_url.as_deref().unwrap_or(&tr.url);
        let track_value = truncate_embed_field_value(&format!("[{}]({})", track_title, track_link));
        embed = embed.field("Track", track_value, false);
        embed = embed.field(
            "Length",
            truncate_embed_field_value(&format_duration(tr.meta.duration)),
            true,
        );
        embed = embed.field(
            "Requested by",
            truncate_embed_field_value(&format!("<@{}>", tr.requested_by)),
            true,
        );
        let thumb = tr
            .meta
            .thumbnail
            .clone()
            .or_else(|| youtube_thumbnail(track_link));
        if let Some(thumbnail) = thumb.as_deref() {
            embed = embed.thumbnail(thumbnail);
        }
    }

    embed
}

/// 再生ステートに合わせてボタン行を生成する。
pub(crate) fn control_components(state: PlayMode) -> Vec<CreateActionRow> {
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
    tracing::info!(guild = %gid, "stop playback requested");
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird 未初期化")?;
    let call = manager.get(gid).ok_or("VC に接続していません")?.clone();

    let flag = transition_flag(&ctx.data().transition_flags, gid);
    flag.store(true, Ordering::Release);
    struct FlagGuard(Arc<AtomicBool>);
    impl Drop for FlagGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let _guard = FlagGuard(flag);

    call.lock().await.stop();
    ctx.data().queues.remove(&gid);
    ctx.data().playing.remove(&gid);
    ctx.data().history.remove(&gid);
    ctx.data().now_playing.remove(&gid);
    Ok(())
}

/// Embed + ボタン付きのコントロールメッセージを送信する。
async fn send_control_message(
    ctx: &Context<'_>,
    gid: GuildId,
    embed: CreateEmbed,
    controls: PlayMode,
) -> Result<Message, Error> {
    tracing::debug!(guild = %gid, controls = ?controls, "sending control message");
    let reply = CreateReply::default()
        .embed(embed)
        .components(control_components(controls));
    let handle = ctx.send(reply).await?;
    let msg = handle.message().await?.into_owned();
    tracing::debug!(guild = %gid, channel = %msg.channel_id, message = %msg.id, "control message sent");
    ctx.data().now_playing.insert(gid, (msg.channel_id, msg.id));
    Ok(msg)
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
    let _ = interaction
        .create_response(ctx.serenity_context(), builder)
        .await;
}

/// ボタン押下に対し、エフェメラルで短い応答を返す。
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

/// ボタン（停止/一時停止/再開/次へ）を処理し、メッセージを更新する。
async fn handle_controls(
    ctx: &Context<'_>,
    gid: GuildId,
    call: Arc<Mutex<Call>>,
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    playing: PlayingMap,
    mut msg: Message,
) -> Result<(), Error> {
    // アイドル時間が経過するまで待ち続け、何か操作があれば締切を伸ばす。
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
            tracing::debug!(
                guild = %gid,
                user = %interaction.user.id,
                owner = %ctx.author().id,
                custom_id = %interaction.data.custom_id,
                "ignored control interaction from non-owner"
            );
            respond_ephemeral(&ctx, &interaction, "この操作はコマンド実行者のみ可能です").await;
            continue;
        }

        // 操作があれば締切を延長する
        deadline = Instant::now() + CONTROL_IDLE_TIMEOUT;

        let custom = interaction.data.custom_id.as_str();
        tracing::debug!(guild = %gid, user = %interaction.user.id, custom_id = %custom, "control interaction");
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
                tracing::info!(guild = %gid, "pause requested");
                if let Some(entry) = playing.get(&gid) {
                    let (handle, req) = entry.value();
                    if matches!(
                        handle.get_info().await.map(|i| i.playing),
                        Ok(PlayMode::Play)
                    ) {
                        let _ = handle.pause();
                        let embed = track_embed("⏸ 一時停止しました", Some(req), None, ACCENT);
                        update_message(
                            &ctx,
                            &interaction,
                            embed,
                            control_components(PlayMode::Pause),
                        )
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
                tracing::info!(guild = %gid, "resume requested");
                if let Some(entry) = playing.get(&gid) {
                    let (handle, req) = entry.value();
                    if matches!(
                        handle.get_info().await.map(|i| i.playing),
                        Ok(PlayMode::Pause)
                    ) {
                        let _ = handle.play();
                        let embed = track_embed("▶ 再生を再開しました", Some(req), None, SUCCESS);
                        update_message(
                            &ctx,
                            &interaction,
                            embed,
                            control_components(PlayMode::Play),
                        )
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
                tracing::info!(guild = %gid, "skip requested");
                // まずは即時に表示を更新して「Interaction failed」を防ぐ（重い処理は後段）。
                let embed = track_embed("⏳ 次の曲を準備しています…", None, None, ACCENT);
                update_message(&ctx, &interaction, embed, Vec::new()).await;

                // 手動skip中は TrackEndHandler の自動遷移を抑止する。
                let flag = transition_flag(&ctx.data().transition_flags, gid);
                flag.store(true, Ordering::Release);
                struct FlagGuard(Arc<AtomicBool>);
                impl Drop for FlagGuard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = FlagGuard(flag);

                if let Some(entry) = playing.get(&gid) {
                    let (handle, _) = entry.value();
                    let _ = handle.stop();
                }
                playing.remove(&gid);

                let res = play_next_from_queue(
                    gid,
                    call.clone(),
                    queues.clone(),
                    playing.clone(),
                    ctx.data().transition_flags.clone(),
                    ctx.data().history.clone(),
                    ctx.serenity_context().http.clone(),
                    ctx.data().now_playing.clone(),
                    3,
                )
                .await?;

                if let Some(started_req) = res.started {
                    tracing::info!(
                        guild = %gid,
                        skipped = res.skipped,
                        remaining = res.remaining,
                        url = %started_req.url,
                        "skip started next track"
                    );
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

                tracing::warn!(
                    guild = %gid,
                    remaining = res.remaining,
                    last_error = ?res.last_error,
                    "skip failed to start next track"
                );
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

    // アイドルタイムアウト後は操作ボタンを無効化して「Interaction failed」を防ぐ
    let _ = msg
        .edit(
            ctx.serenity_context(),
            EditMessage::new().components(Vec::new()),
        )
        .await;
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
    tracing::debug!(
        author = %ctx.author().id,
        has_query = query.is_some(),
        "play command invoked"
    );

    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    tracing::info!(guild = %gid, author = %ctx.author().id, "play command in guild");
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
        tracing::info!(guild = %gid, "resume without query");
        if let Some(h) = current_handle {
            let _ = h.play();
            let embed = track_embed(
                "▶ 再生を再開しました",
                current_req.as_ref(),
                Some("一時停止中のトラックを続きから再生します。".into()),
                SUCCESS,
            );
            let msg = send_control_message(&ctx, gid, embed, PlayMode::Play).await?;
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
        if playlist::is_youtube_playlist_url(&q) {
            tracing::info!(guild = %gid, "expanding youtube playlist");
            ctx.defer().await?;
            match playlist::expand_youtube_playlist(&q, MAX_PLAYLIST_ITEMS).await {
                Ok(urls) => {
                    tracing::info!(guild = %gid, items = urls.len(), "playlist expanded");
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
                        tracing::info!(
                            guild = %gid,
                            added = total,
                            start = position_start,
                            end = position_end,
                            "playlist enqueued while playing"
                        );

                        let embed = track_embed(
                            "📃 プレイリストをキューに追加しました",
                            Some(&preview),
                            Some(format!(
                                "{total} 件を展開しました。キュー #{position_start}〜#{position_end} に追加しました。"
                            )),
                            ACCENT,
                        );
                        let msg = send_control_message(&ctx, gid, embed, current_state).await?;
                        handle_controls(
                            &ctx,
                            gid,
                            call.clone(),
                            queues.clone(),
                            playing.clone(),
                            msg,
                        )
                        .await?;
                        paginate(ctx, &page_slices).await?;
                        return Ok(());
                    } else {
                        let first = reqs.remove(0);
                        {
                            let mut guard = queues.entry(gid).or_default();
                            for r in reqs {
                                guard.push_back(r);
                            }
                        }
                        match play_track_req(
                            gid,
                            call.clone(),
                            queues.clone(),
                            playing.clone(),
                            ctx.data().transition_flags.clone(),
                            ctx.data().history.clone(),
                            ctx.serenity_context().http.clone(),
                            ctx.data().now_playing.clone(),
                            first,
                        )
                        .await
                        {
                            Ok((_handle, started_req)) => {
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
                                    send_control_message(&ctx, gid, embed, PlayMode::Play).await?;
                                handle_controls(
                                    &ctx,
                                    gid,
                                    call.clone(),
                                    queues.clone(),
                                    playing.clone(),
                                    msg,
                                )
                                .await?;
                                paginate(ctx, &page_slices).await?;
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
                    tracing::info!(guild = %gid, position, url = %req.url, "added track to queue while playing");
                    let embed = track_embed(
                        "📥 キューに追加しました",
                        Some(&req),
                        Some(format!(
                            "現在再生中です。キュー #{position} に追加しました。"
                        )),
                        ACCENT,
                    );
                    let msg = send_control_message(&ctx, gid, embed, current_state).await?;
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
                        ctx.data().transition_flags.clone(),
                        ctx.data().history.clone(),
                        ctx.serenity_context().http.clone(),
                        ctx.data().now_playing.clone(),
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
                                send_control_message(&ctx, gid, embed, PlayMode::Play).await?;
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

        let res = play_next_from_queue(
            gid,
            call.clone(),
            queues.clone(),
            playing.clone(),
            ctx.data().transition_flags.clone(),
            ctx.data().history.clone(),
            ctx.serenity_context().http.clone(),
            ctx.data().now_playing.clone(),
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
            let msg = send_control_message(&ctx, gid, embed, PlayMode::Play).await?;
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
    let msg = send_control_message(&ctx, gid, embed, current_state).await?;
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
