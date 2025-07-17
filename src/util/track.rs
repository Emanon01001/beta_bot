use poise::serenity_prelude::{self, UserId};
use songbird::input::{AuxMetadata, Compose, YoutubeDl};

use crate::{get_http_client, util::alias::Error};

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

    /// URL から非同期にメタを引いて TrackRequest を作る
    pub async fn from_url(url: String, requested_by: UserId) -> Result<Self, Error> {
        let mut ytdl = YoutubeDl::new(get_http_client(), url.clone());
        let meta = ytdl.aux_metadata().await.unwrap_or_default();
        Ok(Self {
            url,
            requested_by,
            meta: meta,
        })
    }
}
