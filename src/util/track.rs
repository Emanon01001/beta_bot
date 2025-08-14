use poise::serenity_prelude::{self, UserId};
use songbird::input::{AuxMetadata, Compose, YoutubeDl};
use url::Url;

use crate::{
    get_http_client,
    util::{alias::Error, play::is_youtube},
};

#[derive(Clone, Debug)]
pub struct TrackRequest {
    pub url: String,
    pub requested_by: serenity_prelude::UserId,
    pub meta: AuxMetadata,
}

impl TrackRequest {
    /// メタ情報は未取得の「プレースホルダ」
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
        .user_args(vec!["--ignore-config".into(), "--no-warnings".into()]);

        let meta: AuxMetadata = ytdl
            .aux_metadata()
            .await
            .map_err(|e| {
                "❌ メタデータが取得できませんでした"
            })?;

        let url = meta.source_url.clone().unwrap_or(raw);
        tracing::info!(
            %url,
            title = meta.title.as_deref().unwrap_or(""),
            "metadata obtained"
        );

        Ok(Self { url, requested_by, meta })
    }
}
