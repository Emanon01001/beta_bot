use crate::util::{alias::{Context, Error}, queue::RepeatMode};

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn repeat(
    ctx: Context<'_>,
    #[description = "off | track | queue"] mode: String,
) -> Result<(), Error> {
    todo!("Implement repeat command")
}
