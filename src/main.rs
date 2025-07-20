// src/main.rs ───────────────────────────────────────────
mod commands;
mod handlers;
mod models;
mod util;

use clap::Parser;
use once_cell::sync::Lazy;
use poise::serenity_prelude::{Client, GatewayIntents};
use serde::Deserialize;
use songbird::{Config, SerenityInit};
use std::{path::PathBuf, sync::OnceLock};

use crate::{commands::create_commands::create_commands, models::data::Data, util::alias::Error};

/// ───── CLI 引数定義 ─────
#[derive(Parser)]
struct Cli {
    /// 設定ファイルのパス（未指定なら ./Setting.toml）
    #[arg(long, short, default_value = "Setting.toml")]
    config: PathBuf,
}

#[derive(Deserialize)]
struct ConfigFile {
    token: Tokens,
}
#[derive(Deserialize)]
struct Tokens {
    token: String,
    api_key: String,
}

/// グローバル設定を once で保持
static GLOBAL_CONFIG: Lazy<ConfigFile> = Lazy::new(|| {
    let cli = Cli::parse(); // ← 引数取得
    let contents =
        std::fs::read_to_string(&cli.config).expect("設定ファイルの読み込みに失敗しました");
    toml::from_str(&contents).expect("設定ファイルのパースに失敗しました")
});

pub fn get_http_client() -> reqwest::Client {
    static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP_CLIENT.get_or_init(reqwest::Client::new).clone()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    // ── Poise フレームワーク ──
    let framework = poise::Framework::<Data, Error>::builder()
        .options(poise::FrameworkOptions {
            commands: create_commands(),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("s!".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                println!("Bot is ready!");
                Ok(Data::new())
            })
        })
        .build();

    // ── Songbird 設定 ──
    let songbird_cfg = Config::default().decode_mode(songbird::driver::DecodeMode::Decode);
    let intents = GatewayIntents::all() | GatewayIntents::GUILD_VOICE_STATES;

    // ── Client 起動 ──
    let mut client = Client::builder(&GLOBAL_CONFIG.token.token, intents)
        .framework(framework)
        .register_songbird_from_config(songbird_cfg)
        .await?;

    let shard = tokio::spawn(async move {
        if let Err(why) = client.start().await {
            eprintln!("Client ended: {why:?}");
        }
    });

    tokio::signal::ctrl_c().await.ok();
    println!("Received Ctrl-C, shutting down.");

    shard.abort();
    Ok(())
}
