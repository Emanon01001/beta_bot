# Beta Bot (Discord Music Bot)

軽量な Discord 音楽ボットです。yt-dlp と songbird を使い、スラッシュ／プレフィックスコマンドとメッセージ内ボタンで再生・一時停止・スキップ・停止を操作できます。

## 必要環境
- Rust (stable)
- yt-dlp が PATH にあること
- Discord Bot Token（必須）と Google Gemini API キー（`/exec` 用、任意）

## セットアップ
1) `Setting.toml` を用意してトークンを設定してください（`token` と `api_key`）。このリポジトリに入っている値は差し替えてください。  
2) 依存を取得: `cargo fetch` （または初回 `cargo run` 時に自動取得）  
3) yt-dlp をインストール（例: `pip install -U yt-dlp` など）。

`Setting.toml` で `yt_dlp` セクションを追加すると、cookies や proxy、追加引数を渡せます（例は `src/main.rs` の `ConfigFile` 構造体を参照）。

## 実行
```bash
cargo run --release
# 起動後に「Beta Bot running. Press Ctrl+C to stop.」が出れば OK
```

## 主なコマンド
- `/play <url|検索語>` … 再生開始。メッセージ内ボタンで⏸再開・⏭次へ・⏹停止ができます。
- `/queue [url|検索語]` … 引数なしでキュー確認、指定するとキュー追加。
- `/skip`, `/stop`, `/pause`, `/resume`
- `/join`, `/leave`
- `/insert <url>` … キュー先頭に追加。
- `/remove <index>` … キューから削除。
- `/repeat <Off|Track|Queue>` / `/shuffle <true|false>`
- `/search <query>` … yt-dlp 経由で検索しページ送り表示。
- 補助: `/button_test`, `/pages`, `/exec`（Gemini API 実行例）

## メモ
- ステージチャンネルで使う場合は bot に「話す」を付与し、手を挙げた bot を承認してください。ミュートや音量 0 になっていないかも確認してください。
- ボタン操作は 3 分間有効です。タイムアウト後は再度コマンドを実行してください。

