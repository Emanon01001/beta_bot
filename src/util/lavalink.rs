use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::Deserialize;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    task::JoinHandle,
    time::{Duration, timeout},
};

use crate::{LavalinkSettings, get_http_client};

pub struct LavalinkProcess {
    child: Child,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

#[derive(Deserialize, Debug)]
struct LavalinkInfo {
    #[serde(default)]
    version: Option<LavalinkVersion>,
    #[serde(default)]
    plugins: Vec<LavalinkPlugin>,
}

#[derive(Deserialize, Debug)]
struct LavalinkVersion {
    #[serde(default)]
    semver: Option<String>,
}

#[derive(Deserialize, Debug)]
struct LavalinkPlugin {
    name: String,
}

fn resolve_dir(raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        return p;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(p)
}

fn resolve_file(base: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        return p;
    }
    base.join(p)
}

pub async fn spawn_lavalink(cfg: Option<&LavalinkSettings>) -> Option<LavalinkProcess> {
    let Some(cfg) = cfg else {
        return None;
    };
    if !cfg.enabled {
        return None;
    }
    if !cfg.auto_start {
        tracing::info!("lavalink auto-start disabled by config");
        return None;
    }

    let working_dir = resolve_dir(cfg.working_dir.as_deref().unwrap_or("src/lavalink"));
    let java_raw = cfg.java_path.as_deref().unwrap_or(if cfg!(windows) {
        "jdk-17/bin/java.exe"
    } else {
        "jdk-17/bin/java"
    });
    let jar_raw = cfg.jar_path.as_deref().unwrap_or("Lavalink.jar");
    let java_path = resolve_file(&working_dir, java_raw);
    let jar_path = resolve_file(&working_dir, jar_raw);

    if !working_dir.is_dir() {
        tracing::error!(path = %working_dir.display(), "Lavalink working_dir does not exist");
        return None;
    }
    if !jar_path.is_file() {
        tracing::error!(path = %jar_path.display(), "Lavalink jar file not found");
        return None;
    }

    // If bundled java is missing, fall back to PATH java.
    let program = if java_path.is_file() {
        java_path.clone()
    } else {
        tracing::warn!(
            path = %java_path.display(),
            "bundled java not found; falling back to `java` on PATH"
        );
        PathBuf::from("java")
    };

    let mut cmd = Command::new(&program);
    cmd.current_dir(&working_dir)
        .arg("-jar")
        .arg(&jar_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    tracing::info!(
        working_dir = %working_dir.display(),
        java = %program.display(),
        jar = %jar_path.display(),
        "starting Lavalink process"
    );

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            tracing::error!(error = %err, "failed to spawn Lavalink process");
            return None;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("[lavalink] {line}");
            }
        }
    });
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("[lavalink] {line}");
            }
        }
    });

    Some(LavalinkProcess {
        child,
        stdout_task,
        stderr_task,
    })
}

pub async fn shutdown_lavalink(mut proc: Option<LavalinkProcess>) {
    let Some(mut proc) = proc.take() else {
        return;
    };

    match proc.child.try_wait() {
        Ok(Some(status)) => {
            tracing::info!(status = %status, "Lavalink process already exited");
        }
        Ok(None) => {
            tracing::info!("stopping Lavalink process");
            let _ = proc.child.start_kill();
            match timeout(Duration::from_secs(5), proc.child.wait()).await {
                Ok(Ok(status)) => tracing::info!(status = %status, "Lavalink process stopped"),
                Ok(Err(err)) => tracing::warn!(error = %err, "failed waiting Lavalink process"),
                Err(_) => tracing::warn!("timeout while waiting Lavalink process to stop"),
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to inspect Lavalink process status");
        }
    }

    proc.stdout_task.abort();
    proc.stderr_task.abort();
}

/// Probe Lavalink REST API once on startup to verify configuration.
pub async fn probe_lavalink(cfg: Option<&LavalinkSettings>) {
    let Some(cfg) = cfg else {
        tracing::info!("lavalink config not found; running in songbird mode");
        return;
    };

    if !cfg.enabled {
        tracing::info!("lavalink is disabled by config");
        return;
    }

    let Some(base_url) = cfg.base_url.as_deref().map(str::trim) else {
        tracing::warn!("lavalink is enabled but base_url is missing");
        return;
    };
    if base_url.is_empty() {
        tracing::warn!("lavalink is enabled but base_url is empty");
        return;
    }

    let endpoint = format!("{}/v4/info", base_url.trim_end_matches('/'));
    let timeout_secs = cfg.timeout_secs.unwrap_or(5).clamp(1, 30);
    let mut req = get_http_client().get(&endpoint);
    if let Some(password) = cfg.password.as_deref().map(str::trim) {
        if !password.is_empty() {
            req = req.header("Authorization", password);
        }
    }

    let response = match timeout(Duration::from_secs(timeout_secs), req.send()).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(err)) => {
            tracing::warn!(url = %endpoint, error = %err, "failed to connect to Lavalink");
            return;
        }
        Err(_) => {
            tracing::warn!(url = %endpoint, timeout_secs, "Lavalink probe timed out");
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response body>".to_string());
        tracing::warn!(
            url = %endpoint,
            status = %status,
            body = %body.chars().take(200).collect::<String>(),
            "Lavalink probe failed"
        );
        return;
    }

    match response.json::<LavalinkInfo>().await {
        Ok(info) => {
            let version = info
                .version
                .and_then(|v| v.semver)
                .unwrap_or_else(|| "unknown".to_string());
            let plugin_names = info
                .plugins
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>();
            tracing::info!(
                version = %version,
                plugins = ?plugin_names,
                "Lavalink probe succeeded"
            );
            tracing::info!("note: playback path is still songbird until migration is completed");
        }
        Err(err) => {
            tracing::warn!(error = %err, "Lavalink /v4/info response parse failed");
        }
    }
}
