use crate::util::alias::{Context, Error};

#[poise::command(slash_command, prefix_command)]
pub async fn repeat(
    ctx: Context<'_>,
    #[description = "off / track / queue (省略でトグル)"] mode: Option<String>,
) -> Result<(), Error> {
    Ok(())
}
