use crate::util::{
    alias::{Context, Error},
    track::TrackRequest,
};

#[poise::command(slash_command, guild_only)]
pub async fn insert(ctx: Context<'_>, #[rest] url: String) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    // Data から queues を取得
    let queues = ctx.data().queues.clone(); // Arc<DashMap<…>>
    // entry.or_default() でそのギルドの MusicQueue を初期化
    let mut q = queues.entry(guild_id).or_default();
    q.push_front(TrackRequest::new(url, ctx.author().id));
    ctx.say("優先再生キュー（先頭）に追加しました").await?;
    Ok(())
}