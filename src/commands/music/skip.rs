use crate::util::{
    alias::{Context, Error},
    play::{play_next_from_queue, play_track_req},
    types::TransitionFlags,
};
use poise::serenity_prelude::{Colour, EditMessage};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

fn transition_flag(flags: &TransitionFlags, gid: poise::serenity_prelude::GuildId) -> Arc<AtomicBool> {
    flags
        .entry(gid)
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn skip(
    ctx: Context<'_>,
    #[description = "進む(+) / 戻る(-) の数。省略時は +1"]
    offset: Option<i32>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    // VoiceClient
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird 未初期化")?;
    let call = manager
        .get(guild_id)
        .ok_or("❌ VC に接続していません")?
        .clone();

    // キュー & playing map
    let queues = ctx.data().queues.clone();
    let playing = ctx.data().playing.clone();
    let transition_flags = ctx.data().transition_flags.clone();
    let history = ctx.data().history.clone();
    let http = ctx.serenity_context().http.clone();
    let now_playing = ctx.data().now_playing.clone();

    // 手動skip中は TrackEndHandler の自動遷移を抑止する。
    let flag = transition_flag(&transition_flags, guild_id);
    flag.store(true, Ordering::Release);
    struct FlagGuard(Arc<AtomicBool>);
    impl Drop for FlagGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let _guard = FlagGuard(flag);

    let offset = offset.unwrap_or(1);
    if offset == 0 {
        ctx.say("⚠️ offset は 0 以外を指定してください").await?;
        return Ok(());
    }

    // 現在再生中を停止（戻る時はキュー先頭に戻すことがあるのでreqも拾う）
    let current_req = playing.get(&guild_id).map(|e| e.value().1.clone());
    if let Some(entry) = playing.get(&guild_id) {
        let (handle, _) = entry.value();
        let _ = handle.stop();
    }
    playing.remove(&guild_id);

    if offset < 0 {
        let k = (-offset) as usize;
        let hist_len = history.get(&guild_id).map(|h| h.len()).unwrap_or(0);
        if hist_len < k + 1 {
            ctx.say("⚠️ 戻れる履歴が足りません").await?;
            return Ok(());
        }

        // 履歴から current + k 個を取り出し、target以外をキュー先頭へ戻す
        let (target, mut popped) = {
            let mut hist = history.entry(guild_id).or_default();
            let mut popped = Vec::with_capacity(k + 1);
            for _ in 0..(k + 1) {
                if let Some(t) = hist.pop_back() {
                    popped.push(t);
                }
            }
            let target = popped.pop();
            (target, popped)
        };

        let Some(target) = target else {
            ctx.say("⚠️ 戻れる履歴がありません").await?;
            return Ok(());
        };

        if let Some(cur) = current_req {
            // currentが履歴末尾に無いケースでも、戻った後に「進む」で戻せるようにする
            let already = popped
                .first()
                .is_some_and(|t| t.url == cur.url && t.requested_by == cur.requested_by);
            if !already {
                popped.insert(0, cur);
            }
        }

        if !popped.is_empty() {
            let mut q = queues.entry(guild_id).or_default();
            for tr in popped {
                q.push_front(tr);
            }
        }

        let (_handle, started_req) = play_track_req(
            guild_id,
            call.clone(),
            queues.clone(),
            playing.clone(),
            transition_flags,
            history,
            http,
            now_playing,
            target,
        )
        .await?;

        if let Some((channel_id, message_id)) = ctx.data().now_playing.get(&guild_id).map(|e| *e.value()) {
            let remaining = queues.get(&guild_id).map(|q| q.len()).unwrap_or(0);
            let embed = crate::commands::music::play::track_embed(
                "🎵 再生中",
                Some(&started_req),
                Some(format!("キュー残り {remaining} 件")),
                Colour::new(0x5865F2),
            );
            let components =
                crate::commands::music::play::control_components(songbird::tracks::PlayMode::Play);
            let _ = channel_id
                .edit_message(
                    &ctx.serenity_context().http,
                    message_id,
                    EditMessage::new().embeds(vec![embed]).components(components),
                )
                .await;
        }

        ctx.say(format!("⏮️ {k} 曲戻りました")).await?;
        return Ok(());
    }

    // forward: offset-1 件を捨てて次を再生
    let mut dropped = 0usize;
    if offset > 1 {
        if let Some(mut q) = queues.get_mut(&guild_id) {
            for _ in 0..(offset - 1) {
                if q.pop_next().is_some() {
                    dropped += 1;
                } else {
                    break;
                }
            }
        }
    }

    let res = play_next_from_queue(
        guild_id,
        call.clone(),
        queues.clone(),
        playing.clone(),
        transition_flags,
        history,
        ctx.serenity_context().http.clone(),
        ctx.data().now_playing.clone(),
        3,
    )
    .await?;

    if let Some(started) = res.started {
        if let Some((channel_id, message_id)) =
            ctx.data().now_playing.get(&guild_id).map(|e| *e.value())
        {
            let remaining = queues.get(&guild_id).map(|q| q.len()).unwrap_or(0);
            let embed = crate::commands::music::play::track_embed(
                "🎵 再生中",
                Some(&started),
                Some(format!("キュー残り {remaining} 件")),
                Colour::new(0x5865F2),
            );
            let components =
                crate::commands::music::play::control_components(songbird::tracks::PlayMode::Play);
            let _ = channel_id
                .edit_message(
                    &ctx.serenity_context().http,
                    message_id,
                    EditMessage::new().embeds(vec![embed]).components(components),
                )
                .await;
        }

        let msg = if dropped > 0 {
            format!("⏭️ {dropped} 件スキップして次の曲を再生しました")
        } else {
            "⏭️ 次の曲を再生しました".to_string()
        };
        ctx.say(msg).await?;
        return Ok(());
    }

    ctx.say("❌ スキップできる曲がキューにありません").await?;
    Ok(())
}
