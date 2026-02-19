# beta_bot

Rust 製の Discord ボットです。  
Poise + Songbird + Lavalink 構成で、音楽再生コマンドを中心に実装しています。

## 主な機能
- `/play` で再生開始。再生中パネルにボタン (`pause/resume/skip/stop`) を表示
- `/queue` のページ UI（セレクト + `<< < > >>` + `cancel`）
- YouTube プレイリスト URL の展開追加（最大 50 件）
- `/skip <offset>` で複数曲スキップ、`/skip -N` で履歴から巻き戻し
- `/search` で YouTube 検索結果のページ表示
- 補助コマンド: `chat`, `capstone`, `capinfo`

## 必要環境
- Rust (stable)
- Discord Bot Token
- `yt-dlp` が PATH にあること
- `node` コマンドが利用可能であること（`yt-dlp --js-runtimes node` を使用）
- Lavalink サーバー（本プロジェクトでは実質必須）
  - `auto_start=true` の場合は Java 17 + `Lavalink.jar`

## セットアップ
`Setting.toml` を作成して設定します（`Setting.toml` は `.gitignore` 済み）。

```toml
[token]
token = "YOUR_DISCORD_BOT_TOKEN"
api_key = "YOUR_NANO_GPT_API_KEY" # /chat を使う場合のみ

[yt_dlp]
# 任意: いずれかを指定
# cookies_from_browser = "chrome"
# cookies_file = "cookies.txt"
# proxy = "http://127.0.0.1:7890"
# extra_args = ["--extractor-retries", "3"]

[lavalink]
enabled = true
auto_start = true
base_url = "http://127.0.0.1:2333"
password = "youshallnotpass"
timeout_secs = 5
working_dir = "src/lavalink"
java_path = "jdk-17/bin/java.exe" # Windows 例
jar_path = "Lavalink.jar"
startup_wait_ms = 1500
```

補足:
- `enabled = true` で Lavalink クライアントを初期化します。
- `auto_start = true` の場合、`working_dir` 配下の Java / JAR を使って Lavalink を自動起動します。
- `auto_start = false` の場合は外部 Lavalink を先に起動してください。

## 実行
```bash
cargo run --release
```

別設定ファイルを使う場合:
```bash
cargo run --release -- --config path/to/Setting.toml
```

Windows で `audiopus_sys` など C/CMake ビルドが失敗する場合:
```bash
cargo run --config "env.CMAKE_TOOLCHAIN_FILE=''"
```

## コマンド一覧
Prefix は `s!` です。

| Command | Slash | Prefix | 説明 |
|---|---|---|---|
| `play [query]` | Yes | Yes | 再生開始。`query` 省略時はキュー再生再開や状態表示 |
| `join [channel_id]` | Yes | Yes | ボイスチャンネル参加 |
| `leave` | Yes | Yes | ボイスチャンネル退出 |
| `queue [query]` | Yes | No | キュー表示。`query` 指定時は追加 |
| `insert <url>` | Yes | No | キュー先頭に挿入 |
| `remove <index>` | Yes | Yes | キューから削除（1 始まり） |
| `skip [offset]` | Yes | Yes | 進む/戻る（負数で巻き戻し） |
| `stop` | Yes | Yes | 停止してキューをクリア |
| `pause` | Yes | Yes | 一時停止 |
| `resume` | Yes | Yes | 再開 |
| `repeat <Off/Track/Queue>` | Yes | No | リピート設定 |
| `shuffle <true/false>` | Yes | Yes | シャッフル設定 |
| `search <query> [count]` | Yes | No | YouTube 検索結果を表示 |
| `chat <prompt>` | Yes | Yes | Nano GPT API を使ったチャット |
| `capstone <arch> [syntax] [hide_bytes] <hex>` | Yes | Yes | 逆アセンブル |
| `capinfo <arch> [syntax] [count] <hex>` | Yes | Yes | 命令詳細の解析 |

## 注意点
- ボタン/セレクト操作は基本的にコマンド実行者のみ有効です。
- 再生パネルの操作待ち時間は約 30 分、`/queue` UI は約 5 分でタイムアウトします。
- `Setting.toml` や `cookies.txt` は機密情報を含むためコミットしないでください。
