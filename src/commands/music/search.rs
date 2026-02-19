use crate::{Error, util::alias::Context};
use lavalink_rs::model::{
    search::SearchEngines,
    track::{Track as LavalinkTrack, TrackData, TrackLoadData},
};
use poise::builtins::paginate;
use std::time::Duration;

const PAGE_SIZE: usize = 5;
const MAX_RESULTS: usize = 50;

fn to_duration(length_ms: u64, is_stream: bool) -> Option<Duration> {
    if is_stream || length_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(length_ms))
    }
}

fn format_duration(dur: Option<Duration>) -> String {
    dur.map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
        .unwrap_or_else(|| "??:??".into())
}

fn track_url(track: &TrackData) -> String {
    if let Some(uri) = track.info.uri.as_ref() {
        return uri.clone();
    }
    if track.info.source_name.contains("youtube") {
        return format!("https://youtu.be/{}", track.info.identifier);
    }
    "-".into()
}

fn first_tracks(load: LavalinkTrack) -> Result<Vec<TrackData>, Error> {
    match load.data {
        Some(TrackLoadData::Track(track)) => Ok(vec![track]),
        Some(TrackLoadData::Search(tracks)) => Ok(tracks),
        Some(TrackLoadData::Playlist(playlist)) => Ok(playlist.tracks),
        Some(TrackLoadData::Error(err)) => Err(Error::from(format!(
            "Lavalink search failed: {}",
            err.message
        ))),
        None => Ok(Vec::new()),
    }
}

#[poise::command(slash_command, guild_only)]
pub async fn search(
    ctx: Context<'_>,
    #[rest]
    #[description = "検索キーワード"]
    query: String,
    #[description = "取得件数(1-50)"] count: Option<usize>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let lavalink = ctx
        .data()
        .lavalink
        .clone()
        .ok_or("Lavalink is not enabled in configuration")?;

    let n = count.unwrap_or(5).clamp(1, MAX_RESULTS);
    let identifier = SearchEngines::YouTube
        .to_query(&query)
        .map_err(|e| Error::from(format!("failed to build search query: {e}")))?;
    let loaded = lavalink
        .load_tracks(guild_id, &identifier)
        .await
        .map_err(|e| Error::from(format!("Lavalink search request failed: {e}")))?;
    let mut tracks = first_tracks(loaded)?;
    tracks.truncate(n);

    if tracks.is_empty() {
        ctx.say("❌ 結果が見つかりませんでした").await?;
        return Ok(());
    }

    let page_texts: Vec<String> = tracks
        .chunks(PAGE_SIZE)
        .enumerate()
        .map(|(pi, chunk)| {
            let mut txt = format!(
                "🔎 『{}』の検索結果 ({}/{})\n\n",
                query,
                pi + 1,
                (n + PAGE_SIZE - 1) / PAGE_SIZE
            );
            for (i, track) in chunk.iter().enumerate() {
                let idx = pi * PAGE_SIZE + i + 1;
                let title = if track.info.title.trim().is_empty() {
                    "Unknown"
                } else {
                    track.info.title.as_str()
                };
                let url = track_url(track);
                let dur = format_duration(to_duration(track.info.length, track.info.is_stream));
                txt.push_str(&format!(
                    "{}. **{}**\n▶️ {}\n⏱️ {}\n\n",
                    idx, title, url, dur
                ));
            }
            txt
        })
        .collect();

    let page_slices: Vec<&str> = page_texts.iter().map(String::as_str).collect();
    paginate(ctx, &page_slices).await?;

    Ok(())
}
