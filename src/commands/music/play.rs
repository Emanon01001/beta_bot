use crate::{Error, util::alias::Context};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[rest]
    #[description = "YouTube URL または検索語 (空で再開)"]
    query: Option<String>,
) -> Result<(), Error> {
    let gid = ctx.guild_id().ok_or("サーバー内で実行してください")?;
    crate::commands::music::play_lavalink::run(&ctx, gid, query).await
}
