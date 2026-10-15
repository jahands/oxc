use oxc_allocator::ArenaVec;
use oxc_diagnostics::OxcDiagnostic;
use oxc_formatter_core::{EmbeddedIr, FormatSession, push_text_with_literal_lines};
use oxc_span::Span;

use crate::options::TomlFormatOptions;

/// Format a standalone TOML file.
///
/// # Errors
/// Returns an [`OxcDiagnostic`] when the parse produces any error.
pub fn format(source_text: &str, options: TomlFormatOptions) -> Result<String, OxcDiagnostic> {
    // JHPATCH: Remove leading blank lines with toml.trimLeadingBlankLines
    let (source_text, offset) = trim_leading_blank_lines(source_text, options);
    let parse = parse_for_format(source_text, offset)?;
    Ok(oxc_toml::format(&parse, options.to_oxc_toml()))
}

/// Format `source_text` into IR for embedding into another formatter's document
/// (e.g. TOML front matter, Markdown code blocks).
///
/// The formatted string crosses as text with literal lines (see AGENTS.md "Divergences and the IR goal").
///
/// # Errors
/// Returns an [`OxcDiagnostic`] when the parse produces any error.
pub fn format_to_ir<'a>(
    session: &FormatSession<'a>,
    source_text: &str,
    options: TomlFormatOptions,
) -> Result<EmbeddedIr<'a>, OxcDiagnostic> {
    // JHPATCH: Remove leading blank lines with toml.trimLeadingBlankLines
    let (source_text, offset) = trim_leading_blank_lines(source_text, options);
    let parse = parse_for_format(source_text, offset)?;

    // The IR only carries `\n`, the printer applies the line ending, and the parent owns the trailing newline
    let toml_options =
        oxc_toml::Options { crlf: false, trailing_newline: false, ..options.to_oxc_toml() };
    let formatted = oxc_toml::format(&parse, toml_options);

    let allocator = session.allocator();
    let mut ir = ArenaVec::new_in(&allocator);
    push_text_with_literal_lines(&mut ir, allocator.alloc_str(&formatted), options.indent_width);
    Ok(EmbeddedIr { ir })
}

// JHPATCH: Leading whitespace removal for toml.trimLeadingBlankLines
/// With `trim_leading_blank_lines`, `source_text` without its leading whitespace,
/// and the byte length removed. Taplo drops the indentation of the first line in any case.
fn trim_leading_blank_lines(source_text: &str, options: TomlFormatOptions) -> (&str, u32) {
    if !options.trim_leading_blank_lines {
        return (source_text, 0);
    }
    let trimmed = source_text.trim_start();
    (trimmed, u32::try_from(source_text.len() - trimmed.len()).unwrap_or(u32::MAX))
}

// JHPATCH: `offset` shifts error labels back to the untrimmed source (toml.trimLeadingBlankLines)
/// `oxc_toml::format()` keeps invalid ranges as-is instead of failing,
/// so bail out on any parse error here.
fn parse_for_format(source_text: &str, offset: u32) -> Result<oxc_toml::Parse<'_>, OxcDiagnostic> {
    let parse = oxc_toml::parse(source_text);
    if let Some(error) = parse.errors.first() {
        return Err(OxcDiagnostic::error(format!("Syntax error: {}", error.message))
            .with_label(Span::new(error.range.start + offset, error.range.end + offset)));
    }
    Ok(parse)
}
