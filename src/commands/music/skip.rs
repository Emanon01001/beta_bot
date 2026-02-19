use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn skip(
    ctx: Context<'_>,
    #[description = "進む(+) / 戻る(-) の数。省略時は +1"] offset: Option<i32>,
) -> Result<(), Error> {
    ctx.defer().await?;
    crate::commands::music::skip_lavalink::run(&ctx, offset).await
}
