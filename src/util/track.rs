use poise::serenity_prelude::{self, UserId};
use songbird::input::{AuxMetadata, YoutubeDl};

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
        Self {
            url,
            requested_by,
            meta: AuxMetadata::default(),
        }
    }

    /// URL または検索語から TrackRequest を作成
    pub async fn from_url(raw: String, requested_by: UserId) -> Result<Self, Error> {
        // 1) YouTube URL なら直接 ytdl に渡す
        // 2) それ以外は検索モードで ytsearch1: を使う
        let mut ytdl = if is_youtube(&raw) {
            YoutubeDl::new(get_http_client(), raw.clone())
        } else {
            YoutubeDl::new_search_ytdl_like("yt-dlp", get_http_client(), raw.clone())
                .user_args(vec!["--flat-playlist".into(), "--dump-json".into()])
        };

        // AuxMetadata を取得 :contentReference[oaicite:0]{index=0}
        let meta: AuxMetadata = ytdl
            .search(Some(1))
            .await?
            .next()
            .ok_or("❌ メタデータが取得できませんでした")?;

        // meta.source_url が得られればこれを、なければ raw を
        let real_url = meta.source_url.clone().unwrap_or_else(|| raw.clone());

        println!("TrackRequest: {} -> {}", raw, real_url);

        Ok(Self {
            url: real_url,
            requested_by,
            meta,
        })
    }
}
