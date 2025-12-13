use anyhow::{Result, anyhow};
use capstone::prelude::*;

/// Print register names
pub fn reg_names(cs: &Capstone, regs: &[RegId]) -> String {
    let names: Vec<String> = regs.iter().filter_map(|&x| cs.reg_name(x)).collect();
    names.join(", ")
}

/// Print instruction group names
pub fn group_names(cs: &Capstone, regs: &[InsnGroupId]) -> String {
    let names: Vec<String> = regs.iter().filter_map(|&x| cs.group_name(x)).collect();
    names.join(", ")
}
/// Disassemble and include each instruction's bytes to the left of its address.
pub fn disassemble_with_bytes_column(
    arch: &str,
    bytes: &[u8],
    syntax: Option<&str>,
) -> Result<String> {
    let cs = build_capstone(arch, syntax)?;
    let insns = cs.disasm_all(bytes, 0x1000)?;

    // Precompute hex tokens, chunked lines, max widths, and entries
    let chunk_bytes: usize = 4; // bytes per hex line
    let mut entries: Vec<(Vec<String>, u64, String)> = Vec::new();
    let mut max_hex_len = 0usize; // width of a single hex chunk
    let mut max_addr_hex_len = 0usize;
    for ins in insns.as_ref() {
        let hex_tokens: Vec<String> = ins.bytes().iter().map(|b| format!("{:02X}", b)).collect();
        let mut hex_chunks: Vec<String> = Vec::new();
        for chunk in hex_tokens.chunks(chunk_bytes) {
            let s = chunk.join(" ");
            if s.len() > max_hex_len {
                max_hex_len = s.len();
            }
            hex_chunks.push(s);
        }
        if hex_chunks.is_empty() {
            hex_chunks.push(String::new());
        }

        let addr = ins.address();
        let addr_hex_len = format!("{:x}", addr).len();
        if addr_hex_len > max_addr_hex_len {
            max_addr_hex_len = addr_hex_len;
        }

        let mnem = ins.mnemonic().unwrap_or("?");
        let ops = ins.op_str().unwrap_or("");
        let asm = if ops.is_empty() {
            mnem.to_string()
        } else {
            format!("{} {}", mnem, ops)
        };
        entries.push((hex_chunks, addr, asm));
    }

    // Target max line width for better readability in embeds
    let max_total_width: usize = 120;

    let mut out = String::new();
    for (hex_chunks, addr, asm) in entries {
        let addr_label = format!("0x{:0>width$x}:", addr, width = max_addr_hex_len);
        let first_hex = &hex_chunks[0];
        let prefix = format!(
            "{:<hex_w$}  {} ",
            first_hex,
            addr_label,
            hex_w = max_hex_len
        );
        let cont_prefix_empty_addr = format!(
            "{:hex_w$}  {:addr_w$} ",
            "",
            "",
            hex_w = max_hex_len,
            addr_w = addr_label.len()
        );

        // Wrap ASM with compact hex column to maximize available width
        let avail = if max_total_width > prefix.len() {
            max_total_width - prefix.len()
        } else {
            0
        };
        let wrapped = if avail >= 40 {
            wrap_asm(&asm, avail)
        } else {
            vec![asm.clone()]
        };

        // First line with ASM
        if let Some(first) = wrapped.first() {
            out.push_str(&prefix);
            out.push_str(first);
            out.push('\n');
        } else {
            out.push_str(&prefix);
            out.push('\n');
        }
        // Continuation lines for wrapped ASM
        for line in wrapped.into_iter().skip(1) {
            out.push_str(&cont_prefix_empty_addr);
            out.push_str(&line);
            out.push('\n');
        }
        // Additional hex chunks (no ASM content)
        for chunk in hex_chunks.into_iter().skip(1) {
            let ln = format!(
                "{:<hex_w$}  {:addr_w$} ",
                chunk,
                "",
                hex_w = max_hex_len,
                addr_w = addr_label.len()
            );
            out.push_str(&ln);
            out.push('\n');
        }
    }
    if out.is_empty() {
        out.push_str("<no instructions>");
    }
    Ok(out)
}

/// Assembly-aware wrapping: prefers splitting at ", ", "] ", ") ",
/// or whitespace near the `width` boundary; otherwise avoids splitting tokens.
fn wrap_asm(s: &str, width: usize) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    if s.len() <= width {
        return vec![s.to_string()];
    }

    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    let bytes = s.as_bytes();
    while start < s.len() {
        let remain = &s[start..];
        if remain.len() <= width {
            out.push(remain.to_string());
            break;
        }

        // Candidate split window [start .. start+width]
        let end_hint = start + width;
        let window = &s[start..end_hint];

        // Prefer punctuation boundaries near the end
        let mut split_idx: Option<usize> = None;
        for pat in &[", ", "] ", ") "] {
            if let Some(pos) = window.rfind(pat) {
                split_idx = Some(start + pos + pat.len());
                break;
            }
        }
        // Fallback: last whitespace
        if split_idx.is_none() {
            if let Some(pos) = window.rfind(char::is_whitespace) {
                split_idx = Some(start + pos + 1);
            }
        }
        // As a last resort: don't split this chunk; extend to next boundary
        if split_idx.is_none() {
            // Find next whitespace after width
            let mut i = end_hint;
            while i < s.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= s.len() {
                out.push(remain.to_string());
                break;
            } else {
                split_idx = Some(i + 1);
            }
        }

        let idx = split_idx.unwrap();
        out.push(s[start..idx].trim_end().to_string());
        start = idx;
    }
    out
}

/// Hex variant of `disassemble_with_bytes_column`.
pub fn disassemble_hex_with_bytes_column(
    arch: &str,
    hex: &str,
    syntax: Option<&str>,
) -> Result<String> {
    let bytes = parse_hex_bytes(hex)?;
    disassemble_with_bytes_column(arch, &bytes, syntax)
}

/// Internal: build Capstone for a given `arch` and optional syntax override.
fn build_capstone(arch: &str, syntax: Option<&str>) -> Result<Capstone> {
    build_capstone_with(arch, syntax, false)
}

/// Internal: build Capstone with configurable detail flag.
fn build_capstone_with(arch: &str, syntax: Option<&str>, detail: bool) -> Result<Capstone> {
    let arch_lc = arch.to_ascii_lowercase();
    let mut base = arch_lc.as_str();
    let mut tokens: Vec<&str> = Vec::new();
    if let Some((b, rest)) = arch_lc.split_once(':') {
        base = b;
        tokens = rest.split(':').collect();
    }

    let mut requested_syntax = syntax.map(|s| s.to_ascii_lowercase());
    if requested_syntax.is_none() {
        if tokens.iter().any(|t| *t == "intel") {
            requested_syntax = Some("intel".into());
        } else if tokens.iter().any(|t| *t == "att") {
            requested_syntax = Some("att".into());
        }
    }

    let cs = if matches!(base, "x86_64" | "x64" | "amd64") {
        let mut b = Capstone::new()
            .x86()
            .mode(arch::x86::ArchMode::Mode64)
            .detail(detail);
        match requested_syntax.as_deref() {
            Some("att") => b = b.syntax(arch::x86::ArchSyntax::Att),
            _ => b = b.syntax(arch::x86::ArchSyntax::Intel),
        }
        b.build()?
    } else if matches!(base, "x86" | "i386" | "ia32") {
        let mut b = Capstone::new()
            .x86()
            .mode(arch::x86::ArchMode::Mode32)
            .detail(detail);
        match requested_syntax.as_deref() {
            Some("att") => b = b.syntax(arch::x86::ArchSyntax::Att),
            _ => b = b.syntax(arch::x86::ArchSyntax::Intel),
        }
        b.build()?
    } else if matches!(base, "arm64" | "aarch64") {
        Capstone::new()
            .arm64()
            .mode(arch::arm64::ArchMode::Arm)
            .detail(detail)
            .build()?
    } else if base == "arm" {
        let mode = if tokens.iter().any(|t| *t == "thumb") {
            arch::arm::ArchMode::Thumb
        } else {
            arch::arm::ArchMode::Arm
        };
        Capstone::new().arm().mode(mode).detail(detail).build()?
    } else {
        return Err(anyhow!("unsupported architecture: {}", arch));
    };

    Ok(cs)
}

/// Parse a hex string like "48 89 e5" or "0x4889e5" into bytes.
pub fn parse_hex_bytes(s: &str) -> Result<Vec<u8>> {
    use std::iter::Peekable;
    let mut buf = String::new();
    let mut it: Peekable<std::str::Chars<'_>> = s.trim().chars().peekable();
    while let Some(c) = it.next() {
        match c {
            // skip separators
            c if c.is_ascii_whitespace() || c == ',' || c == '_' || c == ':' => {}
            // strip any 0x/0X prefixes anywhere in the string
            '0' => {
                if matches!(it.peek(), Some('x') | Some('X')) {
                    it.next();
                } else {
                    buf.push('0');
                }
            }
            c if c.is_ascii_hexdigit() => buf.push(c),
            other => return Err(anyhow!("invalid character in hex string: '{}'", other)),
        }
    }
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    if buf.len() % 2 != 0 {
        return Err(anyhow!("hex string must have even length"));
    }
    let mut out = Vec::with_capacity(buf.len() / 2);
    for i in (0..buf.len()).step_by(2) {
        let byte = u8::from_str_radix(&buf[i..i + 2], 16)
            .map_err(|e| anyhow!("invalid hex at {}: {}", i, e))?;
        out.push(byte);
    }
    Ok(out)
}

/// Disassemble and include register read/write and groups for up to `count` instructions.
pub fn inspect_details(
    arch: &str,
    bytes: &[u8],
    syntax: Option<&str>,
    count: usize,
) -> Result<String> {
    let cs = build_capstone_with(arch, syntax, true)?;
    let insns = cs.disasm_all(bytes, 0x1000)?;

    let mut out = String::new();
    for (idx, i) in insns.as_ref().iter().take(count).enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(&i.to_string());
        out.push('\n');

        let detail = cs.insn_detail(i)?;
        let read = reg_names(&cs, detail.regs_read());
        let write = reg_names(&cs, detail.regs_write());
        let groups = group_names(&cs, detail.groups());

        if !read.is_empty() {
            out.push_str(&format!("    read : {}\n", read));
        }
        if !write.is_empty() {
            out.push_str(&format!("    write: {}\n", write));
        }
        if !groups.is_empty() {
            out.push_str(&format!("    groups: {}\n", groups));
        }
    }
    if out.is_empty() {
        out.push_str("<no instructions>");
    }
    Ok(out)
}
