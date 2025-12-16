// Note: Do not force GUI subsystem in release. We manage console visibility
// programmatically to avoid yt-dlp spawning its own console window on Windows.
mod commands;
mod handlers;
mod models;
mod util;

use clap::Parser;
use once_cell::sync::Lazy;
use poise::serenity_prelude::{Client, GatewayIntents};
use serde::Deserialize;
use songbird::{Config, SerenityInit};
use std::{path::PathBuf, process::Command, sync::OnceLock};
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{commands::create_commands::create_commands, models::data::Data, util::alias::Error};

#[cfg(all(windows, not(debug_assertions)))]
#[inline]
fn hide_console_window() {
    // Allocate a console (if present) and hide it, so child console processes
    // (like yt-dlp) attach to this hidden console instead of popping a new one.
    unsafe {
        unsafe extern "system" {
            fn GetConsoleWindow() -> *mut core::ffi::c_void;
            fn ShowWindow(hWnd: *mut core::ffi::c_void, nCmdShow: i32) -> i32;
        }
        const SW_HIDE: i32 = 0;
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[derive(Parser)]
struct Cli {
    #[arg(long, short, default_value = "Setting.toml")]
    config: PathBuf,
}

#[derive(Deserialize, Default, Clone)]
pub struct ConfigFile {
    pub token: Tokens,
    #[serde(default)]
    pub yt_dlp: Option<YtDlpSettings>,
}

#[derive(Deserialize, Default, Clone)]
pub struct Tokens {
    pub token: String,
    pub api_key: String,
}

#[derive(Deserialize, Default, Clone)]
pub struct YtDlpSettings {
    #[serde(default)]
    pub cookies_from_browser: Option<String>,
    #[serde(default)]
    pub cookies_file: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub extra_args: Option<Vec<String>>,
}

/// グローバル設定を once で保持（読込失敗時は空トークンで継続）
pub static GLOBAL_CONFIG: Lazy<ConfigFile> = Lazy::new(|| {
    let cli = Cli::parse();
    let path = cli.config;

    tracing::info!(config = %path.display(), "loading config");
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                config = %path.display(),
                error = %e,
                "設定ファイルの読み込みに失敗しました (デフォルトの空トークンで続行)"
            );
            String::new()
        }
    };

    // 1) Try nested table form: [token] token="..." api_key="..."
    if let Ok(cfg) = toml::from_str::<ConfigFile>(&contents) {
        tracing::info!("config parsed (nested table)");
        return cfg;
    }

    // 2) Fallback: accept flat keys at the root: token="..." api_key="..."
    #[derive(Deserialize, Default)]
    struct FlatConfig {
        #[serde(default)]
        token: String,
        #[serde(default)]
        api_key: String,
    }
    if let Ok(flat) = toml::from_str::<FlatConfig>(&contents) {
        // Also try to read optional yt_dlp section when using flat format
        #[derive(Deserialize, Default)]
        struct MaybeYt {
            #[serde(default)]
            yt_dlp: Option<YtDlpSettings>,
        }
        let yt = toml::from_str::<MaybeYt>(&contents)
            .unwrap_or_default()
            .yt_dlp;
        tracing::info!("config parsed (flat keys)");
        return ConfigFile {
            token: Tokens {
                token: flat.token,
                api_key: flat.api_key,
            },
            yt_dlp: yt,
        };
    }

    // 3) As a last resort, log and return empty tokens
    if !contents.is_empty() {
        tracing::error!("設定ファイルのパースに失敗しました (無効な形式; デフォルトの空トークンで続行)");
    } else {
        tracing::warn!("設定ファイルが空です (デフォルトの空トークンで続行)");
    }
    ConfigFile {
        token: Tokens {
            token: String::new(),
            api_key: String::new(),
        },
        yt_dlp: None,
    }
});

pub fn get_http_client() -> reqwest::Client {
    static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP_CLIENT.get_or_init(reqwest::Client::new).clone()
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_names(true)
        .with_line_number(true)
        .with_file(true)
        .try_init()
        .ok();
}

async fn run_bot(shutdown_rx: oneshot::Receiver<()>) {
    init_tracing();
    tracing::info!("bot task started");

    // Windows 環境では yt-dlp の自己更新をバックグラウンドで実行（ブロッキング回避）
    if cfg!(target_os = "windows") {
        tracing::debug!("spawning yt-dlp self-update task");
        let _ = tokio::task::spawn_blocking(|| {
            #[cfg(windows)]
            use std::os::windows::process::CommandExt;
            #[cfg(windows)]
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            #[allow(unused_mut)]
            let mut cmd = Command::new("yt-dlp");
            cmd.arg("-U");
            #[cfg(windows)]
            {
                let _ = cmd.creation_flags(CREATE_NO_WINDOW).output();
            }
            #[cfg(not(windows))]
            {
                let _ = cmd.output();
            }
        });
    }

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
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                // Ensure slash commands are registered on startup
                tracing::info!(user = %ready.user.name, "registering global commands");
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!(user = %ready.user.name, "bot is ready");
                Ok(Data::new())
            })
        })
        .build();

    // ── Songbird 設定 ──
    let songbird_cfg = Config::default().decode_mode(songbird::driver::DecodeMode::Decode);
    // 必要最小限の Intent のみを購読して負荷を低減
    // - Prefix コマンドのため MESSAGE_CONTENT を有効化
    // - VC 参加検出のため GUILD_VOICE_STATES を有効化
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;

    if GLOBAL_CONFIG.token.token.trim().is_empty() {
        tracing::warn!("discord token is empty (check config); client creation will likely fail");
    }

    // ── Client 起動 ──
    let mut client = match Client::builder(&GLOBAL_CONFIG.token.token, intents)
        .framework(framework)
        .register_songbird_from_config(songbird_cfg)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to create client");
            return;
        }
    };

    let shard_manager = client.shard_manager.clone();

    let client_task = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            tracing::error!(error = ?e, "client task error");
        }
    });

    tokio::select! {
        _ = shutdown_rx => {
            tracing::info!("shutdown signal received; shutting down shards");
            shard_manager.shutdown_all().await;
        }
        _ = client_task => {
            tracing::warn!("client task finished");
        }
    }

    tracing::info!("bot task finished");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Hide the console window on Windows release builds to prevent a separate
    // yt-dlp console from appearing while keeping a shared hidden console.
    #[cfg(all(windows, not(debug_assertions)))]
    hide_console_window();
    // Bot を起動し、Ctrl+C で停止するシンプルな実装に変更
    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    // Bot 実行
    let handle: JoinHandle<()> = tokio::spawn(run_bot(stop_rx));

    init_tracing();
    tracing::info!("Beta Bot running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;

    // 停止シグナル送信して終了を待機
    tracing::info!("Ctrl+C received; sending shutdown");
    let _ = stop_tx.send(());
    let _ = handle.await;

    tracing::info!("shutdown complete");
    Ok(())
}
