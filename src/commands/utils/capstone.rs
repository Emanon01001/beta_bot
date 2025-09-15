use crate::util::alias::{Context, Error};
use crate::util::capstone;
use poise::serenity_prelude::{Colour, CreateEmbed, CreateAttachment};
use poise::CreateReply;
use chrono::Utc;

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn capstone(
    ctx: Context<'_>,
    #[description = "arch: x86_64[:intel|att] | x86[:intel|att] | arm64 | arm[:thumb|arm]"]
    arch: String,
    #[description = "x86 syntax override: intel or att (optional, default: intel)"]
    syntax: Option<String>,
    #[description = "hide raw bytes field (optional, default: false)"]
    hide_bytes: Option<bool>,
    #[rest]
    #[description = "bytes in hex (e.g., 4889e5 or 0x48 0x89 0xe5)"] bytes: String,
) -> Result<(), Error> {
    // Back-compat for prefix form without explicit syntax/hide flags.
    // - If `syntax` looks like x86 syntax, use it.
    // - If `syntax` looks like a boolean (true/false), use it for hide_bytes.
    // - Otherwise, treat it as the first chunk of the hex bytes.
    let (syntax_opt, hide_from_syntax, bytes_str) = match syntax.as_deref() {
        Some(s) if matches!(s.to_ascii_lowercase().as_str(), "intel" | "att") => {
            (Some(s.to_string()), None, bytes)
        }
        Some(s) if matches!(s.to_ascii_lowercase().as_str(), "true" | "false") => {
            let hide = s.eq_ignore_ascii_case("true");
            (None, Some(hide), bytes)
        }
        Some(s) => {
            // Likely the user wrote: s!capstone x86_64 <hex...>
            let combined = format!("{} {}", s, bytes);
            (None, None, combined)
        }
        None => (None, None, bytes),
    };

    let result = match capstone::disassemble_hex_with_bytes_column(
        &arch,
        &bytes_str,
        syntax_opt.as_deref(),
    ) {
        Ok(text) => text,
        Err(e) => format!("error: {}", e),
    };

    // Determine bytes length and pretty hex for info fields
    let (bytes_len, pretty_hex) = match capstone::parse_hex_bytes(&bytes_str) {
        Ok(v) => {
            let hx = v.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
            (v.len(), hx)
        }
        Err(_) => (0, bytes_str.clone()), // fall back to raw input on parse error
    };

    // Figure out syntax label for display (x86 only)
    let arch_lc = arch.to_ascii_lowercase();
    let is_x86 = matches!(arch_lc.as_str(),
        "x86_64" | "x64" | "amd64" | "x86" | "i386" | "ia32");
    let syntax_label = if is_x86 {
        match syntax_opt.as_deref().map(|s| s.to_ascii_lowercase()) {
            Some(s) if s == "att" => Some("ATT".to_string()),
            Some(_) => Some("Intel".to_string()),
            None => {
                // Also respect arch modifiers like x86_64:att
                if arch_lc.contains(":att") { Some("ATT".into()) } else { Some("Intel".into()) }
            }
        }
    } else { None };

    // Prepare embed description with code block and truncate if necessary
    let mut body = result;
    // embed description limit ~4096; keep margin for code fences
    let max_len = 3800usize;
    let mut truncated = false;
    if body.len() > max_len {
        body.truncate(max_len);
        truncated = true;
    }
    let mut desc = String::new();
    desc.push_str("```asm\n");
    desc.push_str(&body);
    if truncated { desc.push_str("\n... (truncated)"); }
    desc.push_str("\n```");

    // Build embed
    let mut embed = CreateEmbed::default();
    embed = embed.title("🧩 Capstone Disassembly");
    embed = embed.colour(Colour::BLITZ_BLUE);
    embed = embed.timestamp(Utc::now());
    embed = embed.description(desc);
    embed = embed.field("Arch", arch.clone(), true);
    if let Some(ref s) = syntax_label { embed = embed.field("Syntax", s, true); }
    embed = embed.field("Bytes", bytes_len.to_string(), true);
    embed = embed.field("Base", "0x1000", true);
    let hide = hide_from_syntax.unwrap_or_else(|| hide_bytes.unwrap_or(false));
    if !hide {
        embed = embed.field("Hex", pretty_hex, false);
    }

    // Compose full text and attach as a file
    let mut file_text = String::new();
    file_text.push_str("# Capstone Disassembly\n");
    file_text.push_str(&format!("Arch: {}\n", arch));
    if let Some(ref s) = syntax_label { file_text.push_str(&format!("Syntax: {}\n", s)); }
    file_text.push_str(&format!("Bytes: {}\n", bytes_len));
    file_text.push_str("Base: 0x1000\n\n");
    // use untruncated body for file; rebuild from original result
    let file_body = match capstone::disassemble_hex_with_bytes_column(&arch, &bytes_str, syntax_opt.as_deref()) {
        Ok(text) => text,
        Err(_) => body.clone(), // fallback to preview
    };
    file_text.push_str(&file_body);
    if !file_text.ends_with('\n') { file_text.push('\n'); }
    let filename = format!("disasm_{}.txt", arch.replace(':', "-"));
    let attachment = CreateAttachment::bytes(file_text.into_bytes(), filename);

    ctx.send(CreateReply::default().embed(embed).attachment(attachment)).await?;
    Ok(())
}

/// Inspect instructions and show registers read/write and groups.
#[poise::command(slash_command, prefix_command, guild_only, rename = "capinfo")]
pub async fn capinfo(
    ctx: Context<'_>,
    #[description = "arch: x86_64[:intel|att] | x86[:intel|att] | arm64 | arm[:thumb|arm]"]
    arch: String,
    #[description = "x86 syntax: intel or att (optional)"] syntax: Option<String>,
    #[description = "number of instructions to inspect (1-5)"] count: Option<u8>,
    #[rest]
    #[description = "bytes in hex (e.g., 4889e5 or 0x48 0x89 0xe5)"] bytes: String,
) -> Result<(), Error> {
    let count = count.unwrap_or(1).clamp(1, 5) as usize;

    // Similar back-compat: if `syntax` isn't a known syntax token, treat it as part of bytes.
    let (syntax_opt, bytes_str) = match syntax.as_deref() {
        Some(s) if matches!(s.to_ascii_lowercase().as_str(), "intel" | "att") => (Some(s.to_string()), bytes),
        Some(s) => (None, format!("{} {}", s, bytes)),
        None => (None, bytes),
    };

    let result = match capstone::inspect_details_hex(&arch, &bytes_str, syntax_opt.as_deref(), count) {
        Ok(text) => text,
        Err(e) => format!("error: {}", e),
    };

    // Compute Hex preview and metadata
    let (bytes_len, pretty_hex) = match capstone::parse_hex_bytes(&bytes_str) {
        Ok(v) => {
            let hx = v.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
            (v.len(), hx)
        }
        Err(_) => (0, bytes_str.clone()),
    };

    // x86 syntax label
    let arch_lc = arch.to_ascii_lowercase();
    let is_x86 = matches!(arch_lc.as_str(), "x86_64" | "x64" | "amd64" | "x86" | "i386" | "ia32");
    let syntax_label = if is_x86 {
        if let Some(s) = syntax_opt.as_deref() {
            if s.eq_ignore_ascii_case("att") { Some("ATT".to_string()) } else { Some("Intel".to_string()) }
        } else if arch_lc.contains(":att") { Some("ATT".into()) } else { Some("Intel".into()) }
    } else { None };

    let mut desc = String::new();
    desc.push_str("```asm\n");
    desc.push_str(&result);
    desc.push_str("\n```");

    let mut embed = CreateEmbed::default();
    embed = embed.title("🧩 Capstone Inspect");
    embed = embed.colour(Colour::BLITZ_BLUE);
    embed = embed.timestamp(Utc::now());
    embed = embed.description(desc);
    embed = embed.field("Arch", arch.clone(), true);
    if let Some(ref s) = syntax_label { embed = embed.field("Syntax", s, true); }
    embed = embed.field("Bytes", bytes_len.to_string(), true);
    embed = embed.field("Base", "0x1000", true);
    embed = embed.field("Hex", pretty_hex, false);

    // Attach full inspect output as file
    let mut file_text = String::new();
    file_text.push_str("# Capstone Inspect\n");
    file_text.push_str(&format!("Arch: {}\n", arch));
    if let Some(ref s) = syntax_label { file_text.push_str(&format!("Syntax: {}\n", s)); }
    file_text.push_str(&format!("Bytes: {}\n", bytes_len));
    file_text.push_str("Base: 0x1000\n\n");
    file_text.push_str(&result);
    if !file_text.ends_with('\n') { file_text.push('\n'); }
    let filename = format!("inspect_{}.txt", arch.replace(':', "-"));
    let attachment = CreateAttachment::bytes(file_text.into_bytes(), filename);

    ctx.send(CreateReply::default().embed(embed).attachment(attachment)).await?;
    Ok(())
}
