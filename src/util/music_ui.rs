use chrono::Utc;
use poise::serenity_prelude::{ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed};
use songbird::tracks::PlayMode;
use url::Url;

use crate::util::track::TrackRequest;

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

/// 秒数を mm:ss 形式に整形する（不明なら "--:--"）。
fn format_duration(dur: Option<std::time::Duration>) -> String {
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
