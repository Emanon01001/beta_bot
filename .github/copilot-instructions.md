# Copilot Instructions for `beta_bot`

## 概要
- **beta_bot** は Rust 製の Discord 音楽ボットです。`poise`/`songbird` フレームワークを利用し、YouTube/SoundCloud などから音楽再生・キュー管理・リピート・シャッフル等の機能を提供します。
- コマンド/イベント/状態管理/再生ロジックが明確に分離されています。

## 主要ディレクトリ・ファイル
- `src/commands/` ... Slashコマンド/Prefixコマンド群。`music/`配下に再生・キュー・VC制御等の音楽系コマンド。
- `src/handlers/` ... Discordイベントハンドラ。`track_end.rs`は曲終了時の自動再生/リピート処理を担当。
- `src/models/data.rs` ... グローバル状態（キュー・再生中トラック）を管理。
- `src/util/` ... キュー/再生/設定/型定義などの補助ロジック。
- `Setting.toml` ... Botトークン/APIキー等の設定ファイル。
- `resources/windows/` ... Windows用バイナリ（`yt-dlp.exe`/`ffmpeg.exe`）。

## アーキテクチャ・データフロー
- `main.rs` で iced GUI からBot起動/停止を制御。`run_bot()`でPoise/Songbirdのセットアップ。
- 各ギルドごとに `DashMap<GuildId, MusicQueue>` でキュー管理、`DashMap<GuildId, (TrackHandle, TrackRequest)>` で再生中トラック管理。
- コマンド実行時、`TrackRequest`生成→`play_track_req`で再生→`TrackEndHandler`で次曲/リピート処理。
- 設定・状態は `once_cell`/`Arc`/`DashMap` でスレッドセーフに共有。

## 開発・ビルド・テスト
- **ビルド**: `cargo build --release`（最適化有効）
- **実行**: `cargo run` または iced GUIから起動
- **依存**: `yt-dlp`/`ffmpeg`（`resources/windows/`に同梱）
- **設定**: `Setting.toml` をプロジェクトルートに配置
- **テスト**: `src/commands/test.rs` などにコマンド単体テストあり

## プロジェクト固有のパターン・注意点
- **コマンド追加**: `src/commands/music/`に新規ファイル→`create_commands.rs`で登録
- **状態管理**: 共有状態は必ず`Arc<DashMap<...>>`でラップ
- **再生ロジック**: `util/play.rs`/`handlers/track_end.rs`で一元管理。直接トラック再生せず`play_track_req`経由で呼び出す
- **設定**: `Setting.toml`のスキーマは`main.rs`参照
- **Windows依存**: バイナリパスは絶対パス/相対パス両対応

## 参考例
- 新コマンド追加例: `src/commands/music/`に`foo.rs`作成→`create_commands.rs`に`commands::music::foo::foo()`を追加
- キュー操作例: `queues.entry(gid).or_default().push_back(req);`

---
このファイルはAIエージェント向けのガイドです。内容に不明点や追加要望があればフィードバックしてください。
