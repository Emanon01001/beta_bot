use crate::util::{
    alias::Error,
    ytdlp::{cookies_args, extra_args_from_config},
};
use serde_json::Value;
use std::time::Duration;
use url::Url;

pub fn is_youtube_playlist_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    if !(host.contains("youtube.com") || host.contains("m.youtube.com") || host.contains("youtu.be"))
    {
        return false;
    }
    url.query_pairs()
        .any(|(k, v)| k == "list" && !v.trim().is_empty())
}

pub async fn expand_youtube_playlist(raw: &str, limit: usize) -> Result<Vec<String>, Error> {
    let limit = limit.max(1);

    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.arg("--ignore-config")
        .arg("--no-warnings")
        .arg("--flat-playlist")
        .arg("--dump-single-json")
        .arg("-4");
    cmd.args(cookies_args());
    cmd.args(extra_args_from_config());
    cmd.arg(raw);

    let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| Error::from("yt-dlp (playlist) がタイムアウトしました"))?
        .map_err(|e| Error::from(format!("yt-dlp (playlist) 実行失敗: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(Error::from(format!(
            "yt-dlp (playlist) が失敗しました: {}",
            err.trim()
        )));
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::from(format!("yt-dlp (playlist) JSON パース失敗: {e}")))?;

    let mut out = Vec::new();
    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::from("yt-dlp (playlist) の entries が見つかりませんでした"))?;

    for entry in entries {
        if out.len() >= limit {
            break;
        }
        let url = entry
            .get("webpage_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                entry.get("url").and_then(|v| v.as_str()).map(|s| {
                    if s.starts_with("http://") || s.starts_with("https://") {
                        s.to_string()
                    } else {
                        format!("https://www.youtube.com/watch?v={s}")
                    }
                })
            })
            .or_else(|| {
                entry.get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| format!("https://www.youtube.com/watch?v={id}"))
            });

        if let Some(u) = url {
            out.push(u);
        }
    }

    if out.is_empty() {
        return Err(Error::from("プレイリストから動画を取得できませんでした"));
    }
    Ok(out)
}

