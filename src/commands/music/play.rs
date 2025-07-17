use dashmap::DashMap;
use poise::serenity_prelude::GuildId;
use songbird::{get as get_songbird, tracks::{PlayMode, TrackHandle}};
use crate::{
    commands::music::join::_join,
    util::{alias::Context, play::play_track_req, queue::MusicQueue, track::TrackRequest},
    Error,
};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "YouTube URL または検索語"]
    #[rest]
    query: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    // --- ギルド／VC 接続を保証 ---
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    _join(&ctx, guild_id, None).await?;

    // --- Songbird の Call を取得 ---
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird 未初期化")?;
    let call = manager
        .get(guild_id)
        .ok_or("❌ VC に接続していません")?
        .clone();                                   // Arc<Mutex<Call>>

    // --- Data の DashMap（Arc）をクローンして保持 ---
    let queues  = ctx.data().queues.clone();        // Arc<DashMap<…>>
    let playing = ctx.data().playing.clone();       // Arc<DashMap<…>>

    // 1) クエリがあればキューへ追加
    if let Some(url) = query {
        let req = TrackRequest::new(url, ctx.author().id);
        queues.entry(guild_id).or_default().push_back(req);
    }

    // 2) 再生中かどうかチェック
    let is_playing = if let Some(handle_ref) = playing.get(&guild_id) {
        let info = handle_ref.value().get_info().await?;
        !info.playing.is_done()
    } else {
        false
    };

    if is_playing {
        ctx.say("🎶 再生中です。キューに追加しました").await?;
        return Ok(());
    }

    // 3) 未再生なら次曲を取り出して再生
    if let Some(mut q) = queues.get_mut(&guild_id) {
        if let Some(next_req) = q.pop_next() {
            // play_track_req(guild_id, call, queues_arc, next_req)
            let handle = play_track_req(guild_id, call.clone(), queues.clone(), next_req).await?;
            playing.insert(guild_id, handle);
            ctx.say("▶️ 再生を開始しました").await?;
            return Ok(());
        }
    }

    ctx.say("❌ キューに曲がありません").await?;
    Ok(())
}
