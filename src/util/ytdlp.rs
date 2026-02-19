use std::{env, path::PathBuf};

use crate::GLOBAL_CONFIG;

/// 設定ファイル([yt_dlp])と環境変数から、yt-dlp に渡す追加引数を構築する。
/// 優先度: 環境変数 > 設定ファイル。
pub fn extra_args_from_config() -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // Env overrides
    let env_cfb = env::var("YTDLP_COOKIES_FROM_BROWSER").ok();
    let env_cfile = env::var("YTDLP_COOKIES_FILE").ok();
    let env_proxy = env::var("YTDLP_PROXY").ok();

    // NOTE: YTDLP_EXTRA_ARGS はスペース区切りで簡易パース（必要なら config 側の配列を推奨）
    if let Ok(extra) = env::var("YTDLP_EXTRA_ARGS") {
        args.extend(extra.split_whitespace().map(|s| s.to_string()));
    }

    // From config (used only if env not present)
    if let Some(yt) = GLOBAL_CONFIG.yt_dlp.as_ref() {
        if env_cfb.is_none() {
            if let Some(ref b) = yt.cookies_from_browser {
                if !b.trim().is_empty() {
                    args.push("--cookies-from-browser".into());
                    args.push(b.clone());
                }
            }
        }
        if env_cfile.is_none() {
            if let Some(ref p) = yt.cookies_file {
                if !p.trim().is_empty() {
                    args.push("--cookies".into());
                    args.push(p.clone());
                }
            }
        }
        if env_proxy.is_none() {
            if let Some(ref pxy) = yt.proxy {
                if !pxy.trim().is_empty() {
                    args.push("--proxy".into());
                    args.push(pxy.clone());
                }
            }
        }
        if let Some(ref extra) = yt.extra_args {
            for a in extra.iter() {
                if !a.trim().is_empty() {
                    args.push(a.clone());
                }
            }
        }
    }

    // Env overrides applied after config
    if let Some(b) = env_cfb {
        if !b.trim().is_empty() {
            args.push("--cookies-from-browser".into());
            args.push(b);
        }
    }
    if let Some(p) = env_cfile {
        if !p.trim().is_empty() {
            args.push("--cookies".into());
            args.push(p);
        }
    }
    if let Some(pxy) = env_proxy {
        if !pxy.trim().is_empty() {
            args.push("--proxy".into());
            args.push(pxy);
        }
    }

    args
}

/// `songbird::input::YoutubeDl::user_args` 用に、
/// ベース引数 + cookies + 設定/環境由来の追加引数を 1 つの配列にまとめる。
pub fn compose_ytdlp_user_args(mut base: Vec<String>) -> Vec<String> {
    base.extend(["--js-runtimes".into(), "node".into()]);
    base.extend(cookies_args());
    base.extend(extra_args_from_config());
    base
}

/// `cookies.txt` を yt-dlp に渡すための固定引数を返す。
/// 優先度: EXE_DIR/cookies.txt -> CWD/cookies.txt
pub fn cookies_args() -> Vec<String> {
    // If user/config already specifies cookies, do not inject defaults.
    if env::var("YTDLP_COOKIES_FROM_BROWSER").is_ok() || env::var("YTDLP_COOKIES_FILE").is_ok() {
        return Vec::new();
    }
    if let Some(yt) = GLOBAL_CONFIG.yt_dlp.as_ref() {
        if yt
            .cookies_from_browser
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
            || yt
                .cookies_file
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty())
        {
            return Vec::new();
        }
    }

    // Fallback: try to find a cookies.txt near the executable first.
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("cookies.txt");
            if p.is_file() {
                return vec!["--cookies".into(), p.to_string_lossy().into_owned()];
            }
        }
    }

    // Compatibility fallback for local runs from source tree.
    let cwd_path = PathBuf::from("cookies.txt");
    if cwd_path.is_file() {
        return vec!["--cookies".into(), cwd_path.to_string_lossy().into_owned()];
    }
    Vec::new()
}
