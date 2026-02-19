use crate::util::{
    alias::{Context, Error},
    lavalink_player::{play_next_from_queue_lavalink, play_track_req_lavalink},
    music_ui::{control_components, track_embed},
    player::ManualTransitionGuard,
};
use poise::serenity_prelude::{Colour, EditMessage};
use songbird::tracks::PlayMode;

pub async fn run(ctx: &Context<'_>, offset: Option<i32>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    let lavalink = ctx
        .data()
        .lavalink
        .clone()
        .ok_or("Lavalink client is not initialized")?;

    let queues = ctx.data().queues.clone();
    let playing = ctx.data().lavalink_playing.clone();
    let transition_flags = ctx.data().transition_flags.clone();
    let history = ctx.data().history.clone();

    let _guard = ManualTransitionGuard::acquire(&transition_flags, guild_id);

    let offset = offset.unwrap_or(1);
    if offset == 0 {
        ctx.say("⚠️ offset は 0 以外を指定してください").await?;
        return Ok(());
    }

    let current_req = playing.get(&guild_id).map(|e| e.value().clone());
    if let Some(player) = lavalink.get_player_context(guild_id) {
        let _ = player.stop_now().await;
    }
    playing.remove(&guild_id);

    if offset < 0 {
        let k = (-offset) as usize;
        let hist_len = history.get(&guild_id).map(|h| h.len()).unwrap_or(0);
        if hist_len < k + 1 {
            ctx.say("⚠️ 戻れる履歴が足りません").await?;
            return Ok(());
        }

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

        let started_req = play_track_req_lavalink(
            guild_id,
            lavalink.clone(),
            playing.clone(),
            history.clone(),
            target,
        )
        .await?;

        if let Some((channel_id, message_id)) =
            ctx.data().now_playing.get(&guild_id).map(|e| *e.value())
        {
            let remaining = queues.get(&guild_id).map(|q| q.len()).unwrap_or(0);
            let embed = track_embed(
                "🎵 再生中",
                Some(&started_req),
                Some(format!("キュー残り {remaining} 件")),
                Colour::new(0x5865F2),
            );
            let components = control_components(PlayMode::Play);
            let _ = channel_id
                .edit_message(
                    &ctx.serenity_context().http,
                    message_id,
                    EditMessage::new()
                        .embeds(vec![embed])
                        .components(components),
                )
                .await;
        }

        ctx.say(format!("⏮️ {k} 曲戻りました")).await?;
        return Ok(());
    }

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

    let res = play_next_from_queue_lavalink(
        guild_id,
        lavalink.clone(),
        queues.clone(),
        playing.clone(),
        history.clone(),
        3,
    )
    .await?;

    if let Some(started) = res.started {
        if let Some((channel_id, message_id)) =
            ctx.data().now_playing.get(&guild_id).map(|e| *e.value())
        {
            let remaining = queues.get(&guild_id).map(|q| q.len()).unwrap_or(0);
            let embed = track_embed(
                "🎵 再生中",
                Some(&started),
                Some(format!("キュー残り {remaining} 件")),
                Colour::new(0x5865F2),
            );
            let components = control_components(PlayMode::Play);
            let _ = channel_id
                .edit_message(
                    &ctx.serenity_context().http,
                    message_id,
                    EditMessage::new()
                        .embeds(vec![embed])
                        .components(components),
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
