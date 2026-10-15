use oxc_formatter_core::{CoreFormatOptions, FormatOptions};
use oxc_formatter_toml::{TomlFormatOptions, TrailingCommas};

use super::super::oxfmtrc::{FormatConfig, TrailingCommaConfig};

// JHPATCH: Doc mentions the toml options
/// Convert `FormatConfig` into `TomlFormatOptions` for `oxc_formatter_toml`.
///
/// Shared options follow Prettier's `prettier-plugin-toml` conventions,
/// TOML-specific options come from `config.toml`.
///
/// NOTE: Pure field translation:
/// `core` comes pre-validated from the config-resolution gate (`validate()`), so this cannot fail.
pub fn to_oxc_formatter_toml(
    config: &FormatConfig,
    core_options: CoreFormatOptions,
) -> TomlFormatOptions {
    let mut options = TomlFormatOptions::default();
    options.apply_core(core_options);

    // [Prettier] trailingComma: "all" | "es5" | "none"
    // `all`/`es5` are indistinguishable for TOML (arrays only check "not none")
    if let Some(trailing_comma) = config.trailing_comma {
        options.trailing_commas = match trailing_comma {
            TrailingCommaConfig::All | TrailingCommaConfig::Es5 => TrailingCommas::Always,
            TrailingCommaConfig::None => TrailingCommas::Never,
        };
    }

    // JHPATCH: Map the toml options to oxc-toml options to match prettier-plugin-toml configs
    let Some(toml) = &config.toml else {
        return options;
    };

    for (field, value) in [
        (&mut options.align_entries, toml.align_entries),
        (&mut options.align_comments, toml.align_comments),
        (&mut options.align_single_comments, toml.align_single_comments),
        (&mut options.array_auto_expand, toml.array_auto_expand),
        (&mut options.array_auto_collapse, toml.array_auto_collapse),
        (&mut options.compact_arrays, toml.compact_arrays),
        (&mut options.compact_inline_tables, toml.compact_inline_tables),
        (&mut options.compact_entries, toml.compact_entries),
        (&mut options.indent_tables, toml.indent_tables),
        (&mut options.indent_entries, toml.indent_entries),
        (&mut options.reorder_keys, toml.reorder_keys),
    ] {
        if let Some(value) = value {
            *field = value;
        }
    }
    if let Some(allowed_blank_lines) = toml.allowed_blank_lines {
        options.allowed_blank_lines = allowed_blank_lines as usize;
    }

    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_options(json: &str) -> TomlFormatOptions {
        let config: FormatConfig = serde_json::from_str(json).unwrap();
        to_oxc_formatter_toml(&config, CoreFormatOptions::default())
    }

    #[test]
    fn test_toml_defaults() {
        assert_eq!(to_options("{}"), TomlFormatOptions::default());
        assert_eq!(to_options(r#"{ "toml": {} }"#), TomlFormatOptions::default());
    }

    #[test]
    fn test_toml_options() {
        let options = to_options(
            r#"{
                "toml": {
                    "alignEntries": true,
                    "alignComments": true,
                    "alignSingleComments": true,
                    "allowedBlankLines": 1,
                    "arrayAutoExpand": false,
                    "arrayAutoCollapse": false,
                    "compactArrays": false,
                    "compactInlineTables": true,
                    "compactEntries": true,
                    "indentTables": true,
                    "indentEntries": true,
                    "reorderKeys": true
                }
            }"#,
        );
        assert_eq!(
            options,
            TomlFormatOptions {
                align_entries: true,
                align_comments: true,
                align_single_comments: true,
                allowed_blank_lines: 1,
                array_auto_expand: false,
                array_auto_collapse: false,
                compact_arrays: false,
                compact_inline_tables: true,
                compact_entries: true,
                indent_tables: true,
                indent_entries: true,
                reorder_keys: true,
                ..TomlFormatOptions::default()
            }
        );
    }
}
