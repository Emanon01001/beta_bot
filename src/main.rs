mod commands;
mod handlers;
mod models;
mod util;

use once_cell::sync::Lazy;
use poise::serenity_prelude::{Client, GatewayIntents, GuildId};
use serde::Deserialize;
use songbird::{Config, SerenityInit};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use crate::{
    commands::create_commands::create_commands,
    models::data::Data,
    util::{alias::Error, config::MusicConfig, queue::MusicQueue},
};

#[derive(Deserialize, Debug)]
struct Database {
    token: Tokens,
}

#[derive(Deserialize, Debug)]
struct Tokens {
    token: String,
    api_key: String,
}

static GLOBAL_DATA: Lazy<Database> = Lazy::new(|| {
    let config_content = std::fs::read_to_string("D:/Programming/Rust/beta_bot/src/Setting.toml")
        .expect("設定ファイルの読み込みに失敗しました");
    toml::from_str(&config_content).expect("設定ファイルのパースに失敗しました")
});

pub fn get_http_client() -> reqwest::Client {
    static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP_CLIENT.get_or_init(reqwest::Client::new).clone()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let framework = poise::Framework::<Data, Error>::builder()
        .options(poise::FrameworkOptions {
            commands: create_commands(),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("s!".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(move |_ctx, _ready, _framework| {
            Box::pin(async move {
                poise::builtins::register_globally(_ctx, &_framework.options().commands)
                    .await
                    .expect("Failed to register commands globally");
                poise::builtins::register_in_guild(
                    _ctx,
                    &_framework.options().commands,
                    GuildId::new(1336765168704557086),
                ) // Replace with your guild ID
                .await
                .expect("Failed to register commands in guild");
                println!("Bot is ready!");
                Ok(Data {
                    music: Arc::new(Mutex::new(MusicQueue::new())),
                    playing: Arc::new(Mutex::new(None)), // TrackHandleの初期化
                })
            })
        })
        .build();

    let songbird_config = Config::default().decode_mode(songbird::driver::DecodeMode::Decode);

    let intents = GatewayIntents::all() | GatewayIntents::GUILD_VOICE_STATES;

    let mut client = Client::builder(&GLOBAL_DATA.token.token, intents)
        .framework(framework)
        .register_songbird_from_config(songbird_config)
        .await
        .unwrap();

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| println!("Client ended: {:?}", why));
    });

    let _signal_err = tokio::signal::ctrl_c().await;
    println!("Received Ctrl-C, shutting down.");
}
