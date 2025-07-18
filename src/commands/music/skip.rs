use crate::util::{
    alias::{Context, Error},
    play::play_track_req,
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
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

    // pop_next
    if let Some(mut q) = queues.get_mut(&guild_id) {
        if let Some(next_req) = q.pop_next() {
            // play_track_req の引数は (guild_id, call, queues_arc, track_req)
            let _ = play_track_req(
                guild_id,
                call.clone(),
                queues.clone(),
                playing.clone(),
                next_req,
            )
            .await?;
            ctx.say("⏭️ スキップして次の曲を再生しました").await?;
            return Ok(());
        }
    }

    ctx.say("❌ スキップできる曲がキューにありません").await?;
    Ok(())
}
