use crate::util::alias::{Context, Error};

use std::sync::Arc;

use poise::serenity_prelude::{self as serenity, Mentionable};
use songbird::Call;
use tokio::sync::Mutex;

pub async fn _join(
    ctx: &Context<'_>,
    guild_id: serenity::GuildId,
    channel_id: Option<serenity::ChannelId>,
) -> Result<Arc<Mutex<Call>>, Error> {
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialised")?
        .clone();

    let connect_to = if let Some(ch) = channel_id {
        ch
    } else {
        let guild = ctx.guild().ok_or("Guild not found")?;
        guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|state| state.channel_id)
            .ok_or("Not in a voice channel")?
    };

    if let Some(call) = manager.get(guild_id) {
        return Ok(call);
    }

    let call = manager.join(guild_id, connect_to).await?;
    ctx.say(format!("Joined {}", connect_to.mention())).await?;

    Ok(call)
}

#[poise::command(slash_command, prefix_command)]
pub async fn join(ctx: Context<'_>, channel_id: Option<serenity::ChannelId>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild ID not found")?;
    let _ = _join(&ctx, guild_id, channel_id).await?;
    Ok(())
}
