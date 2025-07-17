use crate::util::{
    alias::{Context, Error},
    track::TrackRequest,
};

#[poise::command(slash_command, prefix_command)]
pub async fn insert(ctx: Context<'_>, #[rest] query: String) -> Result<(), Error> {
    ctx.defer().await?;
    let req = TrackRequest::from_url(query, ctx.author().id).await?;
    ctx.data().music.lock().await.push_front(req);
    ctx.say("Added to the front of the queue").await?;
    Ok(())
}
