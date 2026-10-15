use oxc_formatter_core::{
    CoreFormatOptions, FormatOptions, IndentStyle, IndentWidth, LineEnding, LineWidth,
};

// JHPATCH: oxc-toml options under prettier-plugin-toml names (toml options); Default is hand-written for oxc-toml's defaults
/// Format options for TOML.
///
/// The shared layout options and `trailing_commas` map the same way as in `prettier-plugin-toml`.
/// <https://github.com/un-ts/prettier/blob/7a4346d5dbf6b63987c0f81228fc46bb12f8692f/packages/toml/src/index.ts#L27-L31>
///
/// The rest are `oxc-toml` (Taplo) options, named after `prettier-plugin-toml`'s.
/// Their defaults are `oxc-toml`'s, which differ from `prettier-plugin-toml`'s
/// for `align_comments`, `align_single_comments` and `allowed_blank_lines`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TomlFormatOptions {
    pub indent_style: IndentStyle,
    pub indent_width: IndentWidth,
    pub line_width: LineWidth,
    pub line_ending: LineEnding,
    pub trailing_commas: TrailingCommas,
    // JHPATCH: toml options passed to oxc-toml
    /// Align consecutive entries vertically.
    pub align_entries: bool,
    /// Align consecutive comments after entries and array items vertically.
    pub align_comments: bool,
    /// Apply `align_comments` to a lone comment too.
    pub align_single_comments: bool,
    /// Expand arrays that exceed the line width to multiple lines.
    pub array_auto_expand: bool,
    /// Collapse arrays that fit in the line width and contain no comments.
    pub array_auto_collapse: bool,
    /// Omit whitespace padding inside single-line arrays.
    pub compact_arrays: bool,
    /// Omit whitespace padding inside inline tables.
    pub compact_inline_tables: bool,
    /// Omit whitespace around `=`.
    pub compact_entries: bool,
    /// Indent subtables that come in order.
    pub indent_tables: bool,
    /// Indent entries under tables.
    pub indent_entries: bool,
    /// Alphabetically reorder keys that are not separated by blank lines.
    pub reorder_keys: bool,
    /// The maximum number of consecutive blank lines.
    pub allowed_blank_lines: usize,
}

// JHPATCH: oxc-toml's defaults for the toml options
impl Default for TomlFormatOptions {
    fn default() -> Self {
        Self {
            indent_style: IndentStyle::default(),
            indent_width: IndentWidth::default(),
            line_width: LineWidth::default(),
            line_ending: LineEnding::default(),
            trailing_commas: TrailingCommas::default(),
            align_entries: false,
            align_comments: false,
            align_single_comments: false,
            array_auto_expand: true,
            array_auto_collapse: true,
            compact_arrays: true,
            compact_inline_tables: false,
            compact_entries: false,
            indent_tables: false,
            indent_entries: false,
            reorder_keys: false,
            allowed_blank_lines: 2,
        }
    }
}

/// Whether a multi-line array gets a trailing comma (TOML 1.0 allows none in inline tables).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TrailingCommas {
    /// Prettier `trailingComma: "es5" | "all"` (default).
    #[default]
    Always,
    /// Prettier `trailingComma: "none"`.
    Never,
}

impl FormatOptions for TomlFormatOptions {
    fn indent_style(&self) -> IndentStyle {
        self.indent_style
    }

    fn indent_width(&self) -> IndentWidth {
        self.indent_width
    }

    fn line_width(&self) -> LineWidth {
        self.line_width
    }

    fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    fn apply_core(&mut self, core: CoreFormatOptions) {
        self.indent_style = core.indent_style;
        self.indent_width = core.indent_width;
        self.line_width = core.line_width;
        self.line_ending = core.line_ending;
    }
}

impl TomlFormatOptions {
    pub(crate) fn to_oxc_toml(self) -> oxc_toml::Options {
        oxc_toml::Options {
            column_width: self.line_width.value() as usize,
            indent_string: if self.indent_style.is_tab() {
                "\t".to_string()
            } else {
                " ".repeat(self.indent_width.value() as usize)
            },
            crlf: self.line_ending.is_carriage_return_line_feed(),
            array_trailing_comma: matches!(self.trailing_commas, TrailingCommas::Always),
            // JHPATCH: Pass the toml options to oxc-toml
            align_entries: self.align_entries,
            align_comments: self.align_comments,
            align_single_comments: self.align_single_comments,
            array_auto_expand: self.array_auto_expand,
            array_auto_collapse: self.array_auto_collapse,
            compact_arrays: self.compact_arrays,
            compact_inline_tables: self.compact_inline_tables,
            compact_entries: self.compact_entries,
            indent_tables: self.indent_tables,
            indent_entries: self.indent_entries,
            reorder_keys: self.reorder_keys,
            allowed_blank_lines: self.allowed_blank_lines,
            // Callers trim it for `insertFinalNewline: false`
            trailing_newline: true,
            ..Default::default()
        }
    }
}
