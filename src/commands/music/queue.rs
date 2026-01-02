use crate::{
    Error,
    util::{alias::Context, playlist, queue::MusicQueue, track::TrackRequest, ytdlp},
};
use dashmap::DashMap;
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption, EditMessage, GuildId,
};
use serde_json::Value;
use songbird::input::AuxMetadata;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use url::Url;

const PAGE_SIZE: usize = 10;
const MAX_PLAYLIST_ITEMS: usize = 50;
const PREFETCH_METADATA_MAX_ITEMS: usize = 50;
const UI_TIMEOUT: Duration = Duration::from_secs(300);

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

fn format_mmss(dur: Option<Duration>) -> String {
    dur.map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
        .unwrap_or_else(|| "--:--".to_string())
}

fn youtube_video_id(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str().unwrap_or_default();
    if host.contains("youtube.com") || host.contains("m.youtube.com") {
        let id = url
            .query_pairs()
            .find_map(|(k, v)| (k == "v").then_some(v))?;
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        return Some(id.to_string());
    }
    if host.contains("youtu.be") {
        let seg = url.path_segments().and_then(|mut s| s.next())?;
        let seg = seg.trim();
        if seg.is_empty() {
            return None;
        }
        return Some(seg.to_string());
    }
    None
}

fn normalize_youtube_key(url: &str) -> Option<String> {
    youtube_video_id(url).map(|id| format!("yt:{id}"))
}

fn short_url(tr: &TrackRequest) -> String {
    let raw_url = tr.meta.source_url.as_deref().unwrap_or(&tr.url);
    if let Some(id) = youtube_video_id(raw_url) {
        return format!("https://youtu.be/{id}");
    }
    raw_url.to_string()
}

fn display_title(tr: &TrackRequest) -> String {
    let raw_url = tr.meta.source_url.as_deref().unwrap_or(&tr.url);
    if let Some(title) = tr
        .meta
        .title
        .as_deref()
        .filter(|t| !t.trim().is_empty() && *t != raw_url)
    {
        return truncate_chars(title, 80);
    }
    if let Some(id) = youtube_video_id(raw_url) {
        return format!("YouTube ({})", truncate_chars(&id, 16));
    }
    truncate_chars(raw_url, 80)
}

fn total_pages(total_items: usize) -> usize {
    ((total_items + PAGE_SIZE - 1) / PAGE_SIZE).max(1)
}

fn page_slice_bounds(page: usize, total_items: usize) -> (usize, usize) {
    let start = page.saturating_mul(PAGE_SIZE);
    let end = (start + PAGE_SIZE).min(total_items);
    (start, end)
}

fn queue_embed(list: &[TrackRequest], page: usize) -> CreateEmbed {
    let total = list.len();
    let pages = total_pages(total);
    let page = page.min(pages.saturating_sub(1));
    let (start, _end) = page_slice_bounds(page, total);

    let mut desc = String::new();
    for (i, tr) in list.iter().skip(start).take(PAGE_SIZE).enumerate() {
        let idx = start + i + 1;
        let dur = format_mmss(tr.meta.duration);
        let title = display_title(tr);
        let url = short_url(tr);
        desc.push_str(&format!("{idx}. `[{dur}]` [{title}]({url})\n"));
    }

    CreateEmbed::default()
        .title(format!("Page {}/{}", page + 1, pages))
        .description(desc)
}

fn select_menu_options(page: usize, pages: usize) -> Vec<CreateSelectMenuOption> {
    // Discord max options is 25. Show a window around the current page.
    let window = pages.min(25);
    let half = window / 2;
    let start = page.saturating_sub(half).min(pages.saturating_sub(window));
    let end = (start + window).min(pages);

    (start..end)
        .map(|p| {
            let label = format!("Page {}/{}", p + 1, pages);
            CreateSelectMenuOption::new(label, p.to_string()).default_selection(p == page)
        })
        .collect()
}

fn queue_components(page: usize, pages: usize) -> Vec<CreateActionRow> {
    let first_disabled = page == 0 || pages <= 1;
    let last_disabled = pages <= 1 || page + 1 >= pages;

    let menu = CreateSelectMenu::new(
        "queue_goto",
        CreateSelectMenuKind::String {
            options: select_menu_options(page, pages),
        },
    )
    .placeholder("選択")
    .min_values(1)
    .max_values(1);

    vec![
        CreateActionRow::SelectMenu(menu),
        CreateActionRow::Buttons(vec![
            CreateButton::new("queue_first")
                .label("<<")
                .style(ButtonStyle::Secondary)
                .disabled(first_disabled),
            CreateButton::new("queue_prev")
                .label("<")
                .style(ButtonStyle::Secondary)
                .disabled(first_disabled),
            CreateButton::new("queue_next")
                .label(">")
                .style(ButtonStyle::Secondary)
                .disabled(last_disabled),
            CreateButton::new("queue_last")
                .label(">>")
                .style(ButtonStyle::Secondary)
                .disabled(last_disabled),
            CreateButton::new("queue_close")
                .label("cancel")
                .style(ButtonStyle::Danger),
        ]),
    ]
}

async fn fetch_ytdlp_metadata(urls: &[String]) -> Result<HashMap<String, AuxMetadata>, Error> {
    if urls.is_empty() {
        return Ok(HashMap::new());
    }

    let started = std::time::Instant::now();
    tracing::debug!(items = urls.len(), "fetching yt-dlp metadata");

    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.arg("--ignore-config")
        .arg("--no-warnings")
        .arg("--skip-download")
        .arg("--dump-json")
        .arg("-4");
    cmd.args(ytdlp::cookies_args());
    cmd.args(ytdlp::extra_args_from_config());
    for u in urls {
        cmd.arg(u);
    }

    let output = tokio::time::timeout(Duration::from_secs(25), cmd.output())
        .await
        .map_err(|_| Error::from("yt-dlp (metadata) がタイムアウトしました"))?
        .map_err(|e| Error::from(format!("yt-dlp (metadata) 実行失敗: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            took_ms = started.elapsed().as_millis(),
            stderr = %err.trim(),
            "yt-dlp metadata command failed"
        );
        return Err(Error::from(format!(
            "yt-dlp (metadata) が失敗しました: {}",
            err.trim()
        )));
    }

    tracing::debug!(
        took_ms = started.elapsed().as_millis(),
        "yt-dlp metadata fetched"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let webpage_url = v
            .get("webpage_url")
            .and_then(|x| x.as_str())
            .or_else(|| v.get("original_url").and_then(|x| x.as_str()))
            .or_else(|| v.get("url").and_then(|x| x.as_str()));
        let Some(webpage_url) = webpage_url else {
            continue;
        };
        let Some(key) = normalize_youtube_key(webpage_url) else {
            continue;
        };

        let mut meta = AuxMetadata::default();
        meta.source_url = Some(webpage_url.to_string());
        meta.title = v
            .get("title")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        meta.thumbnail = v
            .get("thumbnail")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        meta.duration = v
            .get("duration")
            .and_then(|x| x.as_f64())
            .and_then(|d| {
                if d.is_finite() && d > 0.0 {
                    Some(d)
                } else {
                    None
                }
            })
            .map(Duration::from_secs_f64);

        map.insert(key, meta);
    }
    Ok(map)
}

async fn prefetch_queue_metadata(
    queues: Arc<DashMap<GuildId, MusicQueue>>,
    guild_id: GuildId,
    max_items: usize,
) -> Result<(), Error> {
    let Some(snapshot) = queues.get(&guild_id) else {
        return Ok(());
    };

    let mut unique_urls = HashMap::<String, String>::new(); // key -> url
    for tr in snapshot.iter().take(max_items) {
        if tr.meta.title.as_ref().is_some_and(|t| !t.trim().is_empty()) {
            continue;
        }
        let url = tr.meta.source_url.clone().unwrap_or_else(|| tr.url.clone());
        let Some(key) = normalize_youtube_key(&url) else {
            continue;
        };
        unique_urls.entry(key).or_insert(url);
    }
    drop(snapshot);

    if unique_urls.is_empty() {
        return Ok(());
    }

    let urls: Vec<String> = unique_urls.into_values().collect();
    let mut fetched_all: HashMap<String, AuxMetadata> = HashMap::new();

    // yt-dlp は重いので、少量ずつ。
    const CHUNK: usize = 15;
    for chunk in urls.chunks(CHUNK) {
        let m = fetch_ytdlp_metadata(chunk).await?;
        fetched_all.extend(m);
    }

    if fetched_all.is_empty() {
        return Ok(());
    }

    let mut entry = queues.entry(guild_id).or_default();
    let queue = entry.value_mut();
    for tr in queue.queue.iter_mut().take(max_items) {
        if tr.meta.title.as_ref().is_some_and(|t| !t.trim().is_empty()) {
            continue;
        }
        let key_src = tr.meta.source_url.as_deref().unwrap_or(tr.url.as_str());
        let Some(key) = normalize_youtube_key(key_src) else {
            continue;
        };
        if let Some(meta) = fetched_all.get(&key) {
            tr.meta = meta.clone();
        }
    }

    Ok(())
}

async fn ensure_page_metadata(
    queues: &Arc<DashMap<GuildId, MusicQueue>>,
    guild_id: GuildId,
    page: usize,
) -> Result<(), Error> {
    let Some(snapshot) = queues.get(&guild_id) else {
        return Ok(());
    };
    let total = snapshot.len();
    let pages = total_pages(total);
    let page = page.min(pages.saturating_sub(1));
    let (start, end) = page_slice_bounds(page, total);

    let urls_to_fetch = snapshot
        .iter()
        .skip(start)
        .take(PAGE_SIZE)
        .filter(|tr| tr.meta.title.as_ref().is_none_or(|t| t.trim().is_empty()))
        .map(|tr| tr.meta.source_url.clone().unwrap_or_else(|| tr.url.clone()))
        .collect::<Vec<_>>();

    drop(snapshot);

    if urls_to_fetch.is_empty() {
        return Ok(());
    }

    let fetched = match fetch_ytdlp_metadata(&urls_to_fetch).await {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };

    let mut entry = queues.entry(guild_id).or_default();
    let queue = entry.value_mut();
    let total = queue.len();
    let end = end.min(total);
    for idx in start..end {
        let Some(tr) = queue.queue.get_mut(idx) else {
            continue;
        };
        if tr.meta.title.as_ref().is_some_and(|t| !t.trim().is_empty()) {
            continue;
        }
        let key_src = tr.meta.source_url.as_deref().unwrap_or(tr.url.as_str());
        let Some(key) = normalize_youtube_key(key_src) else {
            continue;
        };
        if let Some(meta) = fetched.get(&key) {
            tr.meta = meta.clone();
        }
    }

    Ok(())
}

fn pages_from_urls(urls: &[String], title: &str) -> Vec<String> {
    let pages = total_pages(urls.len());
    let mut out = Vec::with_capacity(pages);
    for p in 0..pages {
        let (start, end) = page_slice_bounds(p, urls.len());
        let mut s = format!("Page {}/{}\n\n", p + 1, pages);
        for (i, url) in urls[start..end].iter().enumerate() {
            let idx = start + i + 1;
            s.push_str(&format!("{idx}. {url}\n"));
        }
        out.push(format!("{title}\n\n{s}"));
    }
    out
}

#[poise::command(slash_command, guild_only)]
pub async fn queue(
    ctx: Context<'_>,
    #[rest]
    #[description = "YouTube URL または検索語 (指定するとキューに追加)"]
    query: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let queues = ctx.data().queues.clone();
    let owner_id = ctx.author().id;
    tracing::info!(
        guild = %guild_id,
        author = %owner_id,
        has_query = query.is_some(),
        "queue command invoked"
    );

    if let Some(q) = query {
        if playlist::is_youtube_playlist_url(&q) {
            tracing::info!(guild = %guild_id, "expanding youtube playlist (queue)");
            ctx.defer().await?;
            match playlist::expand_youtube_playlist(&q, MAX_PLAYLIST_ITEMS).await {
                Ok(urls) => {
                    tracing::info!(guild = %guild_id, items = urls.len(), "playlist expanded (queue)");
                    let reqs = urls
                        .iter()
                        .cloned()
                        .map(|u| TrackRequest::new(u, ctx.author().id))
                        .collect::<Vec<_>>();
                    let total = reqs.len();
                    {
                        let mut guard = queues.entry(guild_id).or_default();
                        for r in reqs {
                            guard.push_back(r);
                        }
                    }
                    tracing::info!(guild = %guild_id, added = total, "playlist enqueued");

                    ctx.say(format!("📃 プレイリストをキューに追加しました ({total}件)"))
                        .await?;

                    let pages = pages_from_urls(&urls, "追加したトラック(URL一覧)");
                    let slices: Vec<&str> = pages.iter().map(String::as_str).collect();
                    poise::builtins::paginate(ctx, &slices).await?;
                }
                Err(e) => {
                    ctx.say(format!("❌ プレイリスト展開に失敗しました: {e}"))
                        .await?;
                }
            }
            return Ok(());
        }

        ctx.defer().await?;
        tracing::info!(guild = %guild_id, "adding single track to queue");
        match TrackRequest::from_url(q, ctx.author().id).await {
            Ok(req) => {
                queues.entry(guild_id).or_default().push_back(req.clone());
                tracing::info!(guild = %guild_id, url = %req.url, "enqueued track");
                let title = truncate_chars(req.meta.title.as_deref().unwrap_or(&req.url), 120);
                ctx.say(format!("✅ キューに追加しました: {title}")).await?;
            }
            Err(e) => {
                tracing::warn!(guild = %guild_id, error = %e, "failed to create track request");
                ctx.say(format!("❌ 追加に失敗しました: {e}")).await?;
            }
        }
        return Ok(());
    }

    let mut list = queues
        .get(&guild_id)
        .map(|q| q.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    if list.is_empty() {
        ctx.say("📭 キューは空です").await?;
        return Ok(());
    }

    // Initial response time limit is 3s; defer and then build UI.
    ctx.defer().await?;

    let mut page = 0usize;

    let mut pages = total_pages(list.len());
    if page >= pages {
        page = pages.saturating_sub(1);
    }

    let reply = CreateReply::default()
        .embed(queue_embed(&list, page))
        .components(queue_components(page, pages));
    let handle = ctx.send(reply).await?;
    let mut msg = handle.message().await?.into_owned();

    // 先読み: 後のページ移動でyt-dlp待ちが発生しにくいようにする（重いのでバックグラウンド）。
    {
        let queues = queues.clone();
        tokio::spawn(async move {
            let _ = prefetch_queue_metadata(queues, guild_id, PREFETCH_METADATA_MAX_ITEMS).await;
        });
    }

    // メタデータ取得は重いので、UI応答をブロックしないようバックグラウンドで行う。
    let generation = Arc::new(AtomicU64::new(0));
    let http = ctx.serenity_context().http.clone();
    {
        let queues = queues.clone();
        let http = http.clone();
        let generation = generation.clone();
        let expected_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
        let msg_id = msg.id;
        let channel_id = msg.channel_id;
        let page0 = page;
        tokio::spawn(async move {
            if generation.load(Ordering::Acquire) != expected_generation {
                return;
            }
            let _ = ensure_page_metadata(&queues, guild_id, page0).await;
            if generation.load(Ordering::Acquire) != expected_generation {
                return;
            }
            let list = queues
                .get(&guild_id)
                .map(|q| q.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            if list.is_empty() {
                return;
            }
            let pages = total_pages(list.len());
            let page0 = page0.min(pages.saturating_sub(1));
            let _ = channel_id
                .edit_message(
                    &http,
                    msg_id,
                    EditMessage::default()
                        .embed(queue_embed(&list, page0))
                        .components(queue_components(page0, pages)),
                )
                .await;
        });
    }

    loop {
        let interaction: Option<ComponentInteraction> =
            poise::serenity_prelude::ComponentInteractionCollector::new(ctx.serenity_context())
                .message_id(msg.id)
                .timeout(UI_TIMEOUT)
                .await;

        let Some(interaction) = interaction else {
            let _ = msg
                .edit(
                    ctx.serenity_context(),
                    EditMessage::default().components(Vec::new()),
                )
                .await;
            break;
        };

        let custom_id = interaction.data.custom_id.as_str();

        if interaction.user.id != owner_id {
            let builder = CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::default()
                    .content("この操作はコマンド実行者のみ可能です")
                    .ephemeral(true),
            );
            let _ = interaction
                .create_response(ctx.serenity_context(), builder)
                .await;
            continue;
        }

        if custom_id == "queue_close" {
            let builder = CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::default()
                    .embeds(vec![queue_embed(&list, page)])
                    .components(Vec::new()),
            );
            let _ = interaction
                .create_response(ctx.serenity_context(), builder)
                .await;
            let _ = msg
                .edit(
                    ctx.serenity_context(),
                    EditMessage::default()
                        .embed(queue_embed(&list, page))
                        .components(Vec::new()),
                )
                .await;
            break;
        }

        match custom_id {
            "queue_first" => page = 0,
            "queue_prev" => page = page.saturating_sub(1),
            "queue_next" => page = page + 1,
            "queue_last" => page = usize::MAX,
            "queue_goto" => {
                if let poise::serenity_prelude::ComponentInteractionDataKind::StringSelect {
                    values,
                } = &interaction.data.kind
                {
                    if let Some(v) = values.first() {
                        if let Ok(p) = v.parse::<usize>() {
                            page = p;
                        }
                    }
                }
            }
            _ => {}
        }

        list = queues
            .get(&guild_id)
            .map(|q| q.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        pages = total_pages(list.len());
        if page >= pages {
            page = pages.saturating_sub(1);
        }

        // まずは即時にページを更新してインタラクション失敗表示を防ぐ。
        let builder = CreateInteractionResponse::UpdateMessage(
            CreateInteractionResponseMessage::default()
                .embeds(vec![queue_embed(&list, page)])
                .components(queue_components(page, pages)),
        );
        if interaction
            .create_response(ctx.serenity_context(), builder)
            .await
            .is_err()
        {
            let _ = msg
                .edit(
                    ctx.serenity_context(),
                    EditMessage::default()
                        .embed(queue_embed(&list, page))
                        .components(queue_components(page, pages)),
                )
                .await;
        }

        // そのページだけメタデータが無ければ取得し、最新ページのときだけ反映する。
        let generation = generation.clone();
        let expected_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
        let queues2 = queues.clone();
        let http2 = http.clone();
        let msg_id = msg.id;
        let channel_id = msg.channel_id;
        let page_for_task = page;
        tokio::spawn(async move {
            if generation.load(Ordering::Acquire) != expected_generation {
                return;
            }
            let _ = ensure_page_metadata(&queues2, guild_id, page_for_task).await;
            if generation.load(Ordering::Acquire) != expected_generation {
                return;
            }
            let list = queues2
                .get(&guild_id)
                .map(|q| q.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            if list.is_empty() {
                return;
            }
            let pages = total_pages(list.len());
            let page = page_for_task.min(pages.saturating_sub(1));
            let _ = channel_id
                .edit_message(
                    &http2,
                    msg_id,
                    EditMessage::default()
                        .embed(queue_embed(&list, page))
                        .components(queue_components(page, pages)),
                )
                .await;
        });
    }

    Ok(())
}
