# Beta Bot (Discord Music Bot)

Discord 音楽ボットです。yt-dlp + songbird で音源を取得・再生し、スラッシュ／プレフィックスコマンドとメッセージ内ボタンで操作できます。

## できること（ざっくり）
- `/play` で「再生中パネル（ボタン付き）」を表示し、曲が変わっても自動で更新
- `/queue` を見やすいページUIで表示（セレクトでページジャンプ、`<< < > >>`、`cancel`）
- YouTube のプレイリストURLを展開してキューに追加（上限あり）
- `/skip [offset]` で任意曲数のスキップ／巻き戻し（履歴ベース）

## 必要環境
- Rust (stable)
- `yt-dlp` が PATH にあること
- Discord Bot Token（必須）
- Google Gemini API キー（`/exec` 用、任意）

## セットアップ
1) `Setting.toml` を作成してトークンを設定してください（`Setting.toml` は `.gitignore` 済み）

例:
```toml
token="YOUR_DISCORD_BOT_TOKEN"
api_key="YOUR_GEMINI_API_KEY" # 任意

# 任意: yt-dlp に渡す追加設定（使っているフィールドは実装に依存）
#[yt_dlp]
#cookies="cookies.txt"
#extra_args=["--proxy", "http://..."]
```

2) 依存を取得: `cargo fetch`（または初回 `cargo run` 時に自動取得）
3) yt-dlp をインストール（例: `pip install -U yt-dlp` など）

## 実行
```bash
cargo run --release
```

Windows で `audiopus_sys` などの C/CMake ビルドが失敗する場合、環境変数 `CMAKE_TOOLCHAIN_FILE` が Android NDK を指していることがあります。その場合は空にして実行してください:
```bash
cargo run --config "env.CMAKE_TOOLCHAIN_FILE=''"
```

## 主なコマンド
- `/play <url|検索語>` … 再生開始（既に再生中ならキュー追加）。ボタンで一時停止/再開/次の曲/停止。
- `/queue [url|検索語|プレイリストURL]` … 引数なしで表示、指定するとキュー追加。
- `/skip [offset]` … `+N` 曲進む / `-N` 曲戻る（例: `/skip 5`, `/skip -1`）。省略時は `+1`。
- `/stop`, `/pause`, `/resume`
- `/join`, `/leave`
- `/insert <url>` … キュー先頭に追加
- `/remove <index>` … キューから削除
- `/repeat <Off|Track|Queue>` / `/shuffle <true|false>`
- `/search <query>` … yt-dlp 経由で検索しページ送り表示

## メモ
- ボタン/セレクトは基本的に「コマンド実行者のみ」操作できます。
- `/play` の再生中パネルは約 30 分、`/queue` のUIは約 5 分でタイムアウトします（タイムアウト後は再実行）。
- ステージチャンネルでは bot に「話す」を付与し、手を挙げた bot を承認してください。
