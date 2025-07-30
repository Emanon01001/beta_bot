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
use tracing_subscriber::util::SubscriberInitExt;
use std::{path::PathBuf, sync::OnceLock};
use tokio::{
    sync::oneshot,
    task::JoinHandle,
};

use iced::{
    Alignment, Color, Element, Length, Shadow, Size, Task, Vector, application,
    widget::{Button, Column, Container, Text, button, column},
    window,
};

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

#[derive(Debug, Clone)]
enum Message {
    ToggleBot,
}

#[derive(Default)]
struct App {
    bot: Option<(JoinHandle<()>, oneshot::Sender<()>)>, // (実行中ハンドル, 停止シグナル)
}

fn update(state: &mut App, _: Message) -> Task<Message> {
    if let Some((_handle, stop_tx)) = state.bot.take() {
        let _ = stop_tx.send(()); // 停止指示を複数タスクに送信
        println!("Bot stopped successfully");
    } else {
        let (tx, rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(run_bot(rx));
        state.bot = Some((handle, tx));
        println!("Bot started successfully");
    }
    Task::none()
}

fn view(app: &App) -> Element<Message> {
    let label = if app.bot.is_some() {
        "Stop Bot"
    } else {
        "Start Bot"
    };
    let btn: Button<_> = button(Text::new(label))
        .padding(12)
        .on_press(Message::ToggleBot)
        .style(|_, _| iced::widget::button::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.86))),
            text_color: Color::WHITE,
            border: iced::Border::default().rounded(16.0),
            shadow: Shadow {
                color: Color::BLACK,
                offset: Vector::new(1.0, 1.0),
                blur_radius: 3.0,
            },
            ..Default::default()
        });

    let col: Column<_> = column![btn].spacing(20).align_x(Alignment::Center);
    Container::new(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

async fn run_bot(shutdown_rx: oneshot::Receiver<()>) {
    tracing_subscriber::FmtSubscriber::new()
        .try_init()
        .ok();

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
        .await
        .expect("Failed to create client");

    let shard_manager = client.shard_manager.clone();

    let client_task = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            eprintln!("Bot error: {:?}", e);
        }
    });

    tokio::select! {
        _ = shutdown_rx => {
            println!("Shutdown signal received, shutting down...");
            shard_manager.shutdown_all().await;
        }
        _ = client_task => {
            println!("Client task finished.");
        }
    }
}

fn main() -> iced::Result {
    application("Simple Bot Toggle", update, view)
        .window(window::Settings {
            size: Size::new(400.0, 200.0),
            ..window::Settings::default()
        })
        .centered()
        .run_with(|| (App::default(), Task::none()))
}
