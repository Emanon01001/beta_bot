use poise::serenity_prelude::{self, UserId};
use songbird::input::{AuxMetadata, Compose, YoutubeDl};
use url::Url;
use std::process::Command;
use tokio::time::{timeout, Duration};

use crate::{
    get_http_client,
    util::{alias::Error, play::is_youtube, ytdlp::{extra_args_from_config, cookies_args}},
};

#[derive(Clone, Debug)]
pub struct TrackRequest {
    pub url: String,
    pub requested_by: serenity_prelude::UserId,
    pub meta: AuxMetadata,
}

impl TrackRequest {
    pub fn new(url: String, requested_by: UserId) -> Self {
        Self { url, requested_by, meta: AuxMetadata::default() }
    }

    #[tracing::instrument(
        name = "TrackRequest::from_url",
        level = "info",
        skip_all,
        fields(raw = %raw, requested_by = %requested_by)
    )]
    pub async fn from_url(raw: String, requested_by: UserId) -> Result<Self, Error> {
        tracing::info!("start resolving track request");
        let parsed = Url::parse(&raw).ok();

        // Preflight: ensure yt-dlp is available on PATH
        // This provides a clearer error to the user instead of a generic metadata failure.
        if let Err(e) = Command::new("yt-dlp").arg("--version").output() {
            return Err(Error::from(format!(
                "yt-dlp が見つかりませんでした ({}). yt-dlp をインストールし、PATH に追加してください。",
                e
            )));
        }

        // 非YouTubeのURLはメタデータ取得しない
        if let Some(ref url) = parsed {
            let is_yt = is_youtube(url.as_str());
            tracing::info!(%url, is_youtube = is_yt, "parsed input as URL");
            if !is_yt {
                return Ok(Self::new(raw, requested_by));
            }
        } else {
            tracing::info!("input is not a URL; treat as YouTube search query");
        }

        // YouTubeのURL、または検索語句(= YouTube検索)のみメタデータ取得
        let mut ytdl = if parsed.is_some() {
            YoutubeDl::new_ytdl_like("yt-dlp", get_http_client(), raw.clone())
        } else {
            YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), raw.clone())
        }
        .user_args(vec!["--ignore-config".into(), "--no-warnings".into()])
        .user_args(cookies_args())
        .user_args(extra_args_from_config());

        // Apply a timeout to guard against yt-dlp hangs; if it fails, continue without metadata
        let fut = ytdl.aux_metadata();
        let meta: AuxMetadata = match timeout(Duration::from_secs(20), fut).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                tracing::warn!(error=%e, "メタデータ取得に失敗しました。メタなしで続行します");
                AuxMetadata::default()
            }
            Err(_) => {
                tracing::warn!("メタデータ取得がタイムアウトしました。メタなしで続行します");
                AuxMetadata::default()
            }
        };

        let url = meta.source_url.clone().unwrap_or(raw);
        tracing::info!(
            %url,
            title = meta.title.as_deref().unwrap_or(""),
            "metadata obtained"
        );

        Ok(Self { url, requested_by, meta })
    }
}