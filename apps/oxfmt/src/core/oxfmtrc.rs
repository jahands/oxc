use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use oxc_config::GlobSet;

use crate::core::{Language, utils};

/// Configuration options for Oxfmt.
///
/// Most options are the same as Prettier's options, but not all of them.
/// In addition, some options are our own extensions.
#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct Oxfmtrc {
    #[serde(flatten)]
    pub format_config: FormatConfig,
    /// File-specific overrides.
    /// When a file matches multiple overrides, the later override takes precedence (array order matters).
    ///
    /// - Default: `[]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<Vec<OxfmtOverrideConfig>>,
    /// Format matched files as the given language, instead of detecting it from the file name.
    /// e.g. Angular templates named `*.html`, or `*.wxml` files formatted as HTML.
    /// When a file matches multiple associations, the later one takes precedence (array order matters).
    ///
    /// - Default: `[]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub associations: Option<Vec<OxfmtAssociationConfig>>,
    /// Ignore files matching these glob patterns.
    /// Patterns use gitignore-style matching, rooted at the directory containing the configuration file.
    /// Files outside that directory cannot be matched; patterns containing `..` are rejected as a configuration error.
    ///
    /// - Default: `[]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_patterns: Option<Vec<String>>,
}

// ---

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OxfmtOverrideConfig {
    /// Glob patterns to match files for this override.
    pub files: GlobSet,
    /// Glob patterns to exclude from this override.
    #[serde(default, skip_serializing_if = "GlobSet::is_empty")]
    pub exclude_files: GlobSet,
    /// Format options to apply for matched files.
    /// Accepts the same options as the top-level format options.
    #[serde(default)]
    pub options: FormatConfig,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OxfmtAssociationConfig {
    /// Glob patterns to match files for this association.
    pub files: GlobSet,
    /// Glob patterns to exclude from this association.
    #[serde(default, skip_serializing_if = "GlobSet::is_empty")]
    pub exclude_files: GlobSet,
    /// The language to format matched files as, named by its Shiki id (as in Markdown code fences).
    pub language: Language,
}

// ---

// NOTE: All fields are typed as `Option` to distinguish between user-specified values and defaults.
#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatConfig {
    // ============================================================================================
    // Prettier compatible options, also used by `oxc_formatter` and TOML formatter
    // ============================================================================================
    /// Indent lines with tabs instead of spaces.
    ///
    /// - Languages: All
    /// - Default: `false`
    /// - Overrides `.editorconfig.indent_style`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_tabs: Option<bool>,
    /// Specify the number of spaces per indentation-level.
    ///
    /// - Languages: All
    /// - Default: `2`
    /// - Overrides `.editorconfig.indent_size` (falls back to `.editorconfig.tab_width`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_width: Option<u8>,
    /// Which end of line characters to apply.
    ///
    /// NOTE: `"auto"` is not supported.
    ///
    /// - Languages: All
    /// - Default: `"lf"`
    /// - Overrides `.editorconfig.end_of_line`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_line: Option<EndOfLineConfig>,
    /// Specify the line length that the printer will wrap on.
    ///
    /// If you don't want line wrapping when formatting Markdown, you can set the `proseWrap` option to disable it.
    ///
    /// - Languages: All
    /// - Default: `100`
    /// - Overrides `.editorconfig.max_line_length`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print_width: Option<u16>,

    /// Use single quotes instead of double quotes.
    ///
    /// For JSX, you can set the `jsxSingleQuote` option.
    ///
    /// - Languages: JS, JSX, TS, TSX, CSS, Less, SCSS, Markdown, MDX, YAML, Handlebars, Svelte, Astro
    /// - Default: `false`
    /// - Overrides `.editorconfig.quote_type`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_quote: Option<bool>,
    /// Use single quotes instead of double quotes in JSX.
    ///
    /// - Languages: JSX, TSX
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsx_single_quote: Option<bool>,
    /// Change when properties in objects are quoted.
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: `"as-needed"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_props: Option<QuotePropsConfig>,
    /// Print trailing commas wherever possible in multi-line comma-separated syntactic structures.
    ///
    /// A single-line array, for example, never gets trailing commas.
    ///
    /// - Languages: JS, JSX, TS, TSX, JSONC, JSON5, TOML, CSS, Less, SCSS, YAML
    /// - Default: `"all"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_comma: Option<TrailingCommaConfig>,
    /// Print semicolons at the ends of statements.
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semi: Option<bool>,
    /// Include parentheses around a sole arrow function parameter.
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: `"always"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_parens: Option<ArrowParensConfig>,
    /// Print spaces between brackets in object literals.
    ///
    /// - Languages: JS, JSX, TS, TSX, JSON, JSONC, JSON5, GraphQL, YAML
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bracket_spacing: Option<bool>,
    /// Put the `>` of a multi-line element at the end of the last line,
    /// instead of being alone on the next line (does not apply to self-closing elements).
    ///
    /// - Languages: JSX, TSX, HTML, Angular, Vue, MJML, Svelte, Astro
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bracket_same_line: Option<bool>,
    /// How to wrap object literals when they could fit on one line or span multiple lines.
    ///
    /// By default, formats objects as multi-line if there is a newline prior to the first property.
    /// Authors can use this heuristic to contextually improve readability, though it has some downsides.
    ///
    /// - Languages: JS, JSX, TS, TSX, JSON, JSONC, JSON5
    /// - Default: `"preserve"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_wrap: Option<ObjectWrapConfig>,
    /// Enforce single attribute per line.
    ///
    /// - Languages: JSX, TSX, HTML, Angular, Vue, MJML, Svelte, Astro
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_attribute_per_line: Option<bool>,
    /// When expressions wrap lines, print operators at the start of new lines (`"start"`)
    /// or at the end of previous lines (`"end"`).
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: `"end"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental_operator_position: Option<OperatorPositionConfig>,

    // NOTE: This experimental option is not yet supported.
    // Reject at deserialize time so all entry paths (base / overrides / NAPI `resolve()`) are covered uniformly.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "reject_experimental_ternaries",
        default
    )]
    #[schemars(skip)]
    pub experimental_ternaries: Option<bool>,

    /// Control whether to format embedded parts in the file.
    /// For example:
    /// - CSS-in-JS: template literal
    /// - JS-in-Vue: `<script>` block
    /// - JS-in-Markdown: code fence
    /// - YAML-in-CSS/Markdown: front matter
    ///
    /// With `"off"`, these parts are kept as-is.
    ///
    /// NOTE: Some languages behave differently.
    /// For Svelte, formatting fails with `"off"`, a limitation of `prettier-plugin-svelte`.
    /// For Astro, the frontmatter is still formatted, but by Prettier instead of Oxfmt (use `astro.skipFrontmatter` to keep it as-is).
    ///
    /// - Languages: JS, JSX, TS, TSX, CSS, SCSS, Less, HTML, Vue, Angular, Svelte, Astro, Markdown, MDX
    /// - Default: `"auto"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedded_language_formatting: Option<EmbeddedLanguageFormattingConfig>,

    /// How to wrap prose.
    ///
    /// - `"always"`: Wrap prose to the print width
    /// - `"never"`: Put each prose block on a single line, relying on editor/viewer soft wrapping
    /// - `"preserve"`: Keep wrapping as-is
    ///
    /// By default, wrapping is preserved, since some services use a linebreak-sensitive renderer (e.g. GitHub comments, BitBucket).
    ///
    /// - Languages: Markdown, MDX, YAML
    /// - Default: `"preserve"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_wrap: Option<ProseWrapConfig>,

    // ============================================================================================
    // Prettier compatible options and only used by Prettier
    // ============================================================================================
    /// Specify the global whitespace sensitivity.
    ///
    /// - Languages: HTML, Angular, Vue, Handlebars, Svelte
    /// - Default: `"css"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_whitespace_sensitivity: Option<HtmlWhitespaceSensitivityConfig>,
    /// Whether or not to indent the code inside `<script>` and `<style>` tags in Vue files.
    ///
    /// - Languages: Vue
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vue_indent_script_and_style: Option<bool>,

    // ============================================================================================
    // Below are our own extensions, handled by Oxfmt
    // ============================================================================================
    /// Whether to insert a final newline at the end of the file.
    ///
    /// - Languages: All
    /// - Default: `true`
    /// - Overrides `.editorconfig.insert_final_newline`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_final_newline: Option<bool>,

    /// Sort import statements.
    ///
    /// Uses a similar algorithm to [eslint-plugin-perfectionist/sort-imports](https://perfectionist.dev/rules/sort-imports).
    ///
    /// Pass `true` or an object to enable, `false` to disable.
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: Disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "experimentalSortImports")]
    pub sort_imports: Option<SortImportsUserConfig>,

    // JHPATCH: Point to sortPackageJson.preset for prettier-plugin-packagejson's order
    /// Sort `package.json` keys.
    ///
    /// The default order is NOT compatible with [prettier-plugin-packagejson](https://github.com/matzkoh/prettier-plugin-packagejson),
    /// but we believe it is clearer and easier to navigate.
    /// Set `preset` to `"sort-package-json"` for the order prettier-plugin-packagejson produces.
    ///
    /// - Languages: JSON (`package.json` only)
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "experimentalSortPackageJson")]
    pub sort_package_json: Option<SortPackageJsonUserConfig>,

    /// Sort Tailwind CSS classes.
    ///
    /// Uses the same algorithm as [prettier-plugin-tailwindcss](https://github.com/tailwindlabs/prettier-plugin-tailwindcss).
    /// Option names omit the `tailwind` prefix used in the original plugin (e.g. `config` instead of `tailwindConfig`).
    ///
    /// Pass `true` or an object to enable, `false` to disable.
    ///
    /// - Languages: JS, JSX, TS, TSX, HTML, Vue, Angular, Handlebars, CSS, SCSS, Less, Svelte, Astro
    /// - Default: Disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "experimentalTailwindcss")]
    pub sort_tailwindcss: Option<SortTailwindcssUserConfig>,

    /// Enable JSDoc comment formatting.
    ///
    /// Normalizes JSDoc comments: tag aliases are canonicalized, descriptions are capitalized,
    /// long lines are wrapped, and short comments are collapsed to single-line.
    ///
    /// Pass `true` or an object to enable, `false` to disable.
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: Disabled
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub jsdoc: Option<JsdocUserConfig>,

    // JHPATCH: prettierCompat option to follow Prettier 3.9's union layout
    /// Follow a newer Prettier version's output where Oxfmt deliberately keeps an earlier Prettier style.
    ///
    /// - `"3.9"`: a union type that does not fit on the line first retries flat on the indented next line,
    ///   and only then breaks one member per line; a union in a conditional type branch starts behind `? ` / `: `
    ///
    /// - Languages: JS, JSX, TS, TSX
    /// - Default: Disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prettier_compat: Option<PrettierCompatConfig>,

    /// Options for `prettier-plugin-svelte`.
    ///
    /// Pass `true` or an object to enable `.svelte` file formatting, `false` to disable (handy in overrides).
    /// Setting `true` resets to defaults, dropping options inherited from a parent scope.
    ///
    /// NOTE: `prettier-plugin-svelte` requires the `svelte` package (`svelte/compiler`) at runtime.
    /// Oxfmt does NOT bundle it, so install it in your project, otherwise formatting fails.
    ///
    /// - Languages: Svelte
    /// - Default: Disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svelte: Option<SvelteUserConfig>,

    /// Options for `prettier-plugin-astro`.
    ///
    /// Pass `true` or an object to enable `.astro` file formatting, `false` to disable (handy in overrides).
    /// Setting `true` resets to defaults, dropping options inherited from a parent scope.
    ///
    /// NOTE: `prettier-plugin-astro` requires the `@astrojs/compiler-rs` package at runtime.
    /// Oxfmt does NOT bundle it, so install it in your project, otherwise formatting fails.
    ///
    /// - Languages: Astro
    /// - Default: Disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub astro: Option<AstroUserConfig>,

    // JHPATCH: markdown options (markdown.lazyCodeSpanContinuation)
    /// Options for Markdown files and Markdown embedded in other languages.
    ///
    /// - Languages: Markdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<MarkdownConfig>,

    // JHPATCH: toml options to match prettier-plugin-toml configs
    /// Options for TOML files and TOML embedded in other languages (front matter, Markdown code blocks).
    ///
    /// Option names follow [prettier-plugin-toml](https://github.com/un-ts/prettier/tree/master/packages/toml),
    /// but unset options keep Taplo's defaults, which differ from the plugin's for
    /// `alignComments`, `alignSingleComments` and `allowedBlankLines`.
    ///
    /// - Languages: TOML
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toml: Option<TomlConfig>,
}

impl FormatConfig {
    /// Whether embedded-language formatting is enabled by this config
    /// (`embeddedLanguageFormatting` defaults to `"auto"`; only an explicit `"off"` disables it).
    /// Consumers reach this through `ResolvedDispatchConfig::is_embedded_formatting_enabled`,
    /// which owns the never-diverge invariant.
    pub fn is_embedded_formatting_enabled(&self) -> bool {
        !matches!(self.embedded_language_formatting, Some(EmbeddedLanguageFormattingConfig::Off))
    }

    /// Whether `prettier-plugin-svelte` is enabled by this config.
    ///
    /// Enabled when `svelte` is set to `true` or an object;
    /// disabled when unset or `false`.
    pub fn is_svelte_enabled(&self) -> bool {
        matches!(self.svelte, Some(SvelteUserConfig::Bool(true) | SvelteUserConfig::Object(_)))
    }

    /// Whether `prettier-plugin-astro` is enabled by this config.
    ///
    /// Enabled when `astro` is set to `true` or an object;
    /// disabled when unset or `false`.
    pub fn is_astro_enabled(&self) -> bool {
        matches!(self.astro, Some(AstroUserConfig::Bool(true) | AstroUserConfig::Object(_)))
    }

    // JHPATCH: Enables oxc_formatter for JS in MDX when sortImports is set (sortImports in MDX)
    /// Whether import sorting is enabled by this config.
    ///
    /// Enabled when `sortImports` is set to `true` or an object;
    /// disabled when unset or `false`.
    pub fn is_sort_imports_enabled(&self) -> bool {
        matches!(
            self.sort_imports,
            Some(SortImportsUserConfig::Bool(true) | SortImportsUserConfig::Object(_))
        )
    }

    /// Whether Tailwind class sorting is enabled by this config.
    ///
    /// Enabled when `sortTailwindcss` is set to `true` or an object;
    /// disabled when unset or `false`.
    pub fn is_tailwind_enabled(&self) -> bool {
        matches!(
            self.sort_tailwindcss,
            Some(SortTailwindcssUserConfig::Bool(true) | SortTailwindcssUserConfig::Object(_))
        )
    }

    /// Resolve relative tailwind paths (`config`, `stylesheet`) to absolute paths.
    /// Otherwise, the plugin tries to resolve the Prettier's configuration file, not Oxfmt's.
    /// <https://github.com/tailwindlabs/prettier-plugin-tailwindcss/blob/125a8bc77639529a5a0c7e4e8a02174d7ed2d70b/src/config.ts#L50-L54>
    pub fn resolve_tailwind_paths(&mut self, base_dir: &Path) {
        let Some(SortTailwindcssUserConfig::Object(ref mut tw)) = self.sort_tailwindcss else {
            return;
        };

        for path_field in [&mut tw.config, &mut tw.stylesheet] {
            let Some(path_str) = path_field.as_ref() else {
                continue;
            };

            let path = Path::new(path_str);
            if path.is_relative() {
                *path_field = Some(
                    utils::normalize_relative_path(base_dir, path).to_string_lossy().to_string(),
                );
            }
        }
    }

    // JHPATCH: Record the directory the project prettier-plugin-astro is resolved from (astro.useProjectPlugin)
    /// Record `base_dir` as the directory `prettier-plugin-astro` is resolved from,
    /// when `astro.useProjectPlugin` is enabled.
    pub fn resolve_astro_plugin_dir(&mut self, base_dir: &Path) {
        if let Some(AstroUserConfig::Object(ref mut astro)) = self.astro
            && astro.use_project_plugin == Some(true)
        {
            astro.project_plugin_dir = Some(base_dir.to_path_buf());
        }
    }

    /// Merge another `FormatConfig`, overwriting only fields that are `Some<T>`.
    ///
    /// # Panics
    /// Panics if serialization/deserialization fails,
    /// which should never happen for valid `FormatConfig` structs.
    pub fn merge(&mut self, other: &Self) {
        let base = serde_json::to_value(&*self).unwrap();
        let overlay = serde_json::to_value(other).unwrap();
        let merged = json_deep_merge(base, overlay);
        *self = serde_json::from_value(merged).unwrap();
    }
}

// ---

fn reject_experimental_ternaries<'de, D>(d: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<bool>::deserialize(d)?;
    if v.is_some() {
        return Err(serde::de::Error::custom("Unsupported option: `experimentalTernaries`"));
    }
    Ok(v)
}

// ---

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EndOfLineConfig {
    Lf,
    Crlf,
    Cr,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum QuotePropsConfig {
    AsNeeded,
    Consistent,
    Preserve,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TrailingCommaConfig {
    All,
    Es5,
    None,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ArrowParensConfig {
    Always,
    Avoid,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ObjectWrapConfig {
    Preserve,
    Collapse,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OperatorPositionConfig {
    Start,
    End,
}

// JHPATCH: prettierCompat values
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
pub enum PrettierCompatConfig {
    #[serde(rename = "3.9")]
    V3_9,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddedLanguageFormattingConfig {
    Auto,
    Off,
}

// ---

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProseWrapConfig {
    Always,
    Never,
    Preserve,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HtmlWhitespaceSensitivityConfig {
    Css,
    Strict,
    Ignore,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SortImportsUserConfig {
    Bool(bool),
    Object(SortImportsConfig),
}

impl SortImportsUserConfig {
    pub fn into_config(self) -> Option<SortImportsConfig> {
        match self {
            Self::Bool(true) => Some(SortImportsConfig::default()),
            Self::Bool(false) => None,
            Self::Object(config) => Some(config),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct SortImportsConfig {
    /// Use empty lines to separate imports into logical groups.
    ///
    /// When `true`, imports are not sorted across an empty line.
    ///
    /// ```js
    /// import { b1, b2 } from 'b'
    ///
    /// import { a } from 'a'
    /// import { c } from 'c'
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_by_newline: Option<bool>,
    /// Use comments to separate imports into logical groups.
    ///
    /// When `true`, all comments are treated as delimiters.
    ///
    /// ```js
    /// import { b1, b2 } from 'b'
    /// // PARTITION
    /// import { a } from 'a'
    /// import { c } from 'c'
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_by_comment: Option<bool>,
    // JHPATCH: sortImports.partitionBySideEffect option to match @ianvs/prettier-plugin-sort-imports
    /// Use side effect imports to separate imports into logical groups.
    ///
    /// When `true`, imports are not sorted across a side effect import that keeps its position
    /// (see `sortSideEffects`), or across an import suppressed with `prettier-ignore` / `oxfmt-ignore`.
    /// Suppressed imports get the same blank lines as side effect imports.
    /// An import with an empty `{}` and no comments (`import {} from 'a'`) is printed as the side effect import `import 'a'`,
    /// as `@ianvs/prettier-plugin-sort-imports` does.
    ///
    /// If any group boundary inserts newlines (`newlinesBetween: true` or a `{ "newlinesBetween": true }` marker),
    /// a run of side effect imports also gets a blank line after it, and one before it.
    /// The blank line before is left out when the run starts with a comment
    /// and no group boundary after the previous import's group inserts newlines.
    ///
    /// ```js
    /// import { b } from 'b'
    /// import { c } from 'c'
    ///
    /// import './setup'
    ///
    /// import { a } from 'a'
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_by_side_effect: Option<bool>,
    /// Sort side effect imports.
    ///
    /// Disabled by default for safety reasons.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_side_effects: Option<bool>,
    /// Sort in ascending or descending order.
    ///
    /// - Default: `"asc"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<SortOrderConfig>,
    /// Ignore case when sorting.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_case: Option<bool>,
    // JHPATCH: sortImports.collation option to match @ianvs/prettier-plugin-sort-imports
    /// How import sources and specifier names are compared.
    ///
    /// - `"natural"`: Code point order, with digit runs compared as numbers.
    /// - `"intl"`: The order of `a.localeCompare(b, 'en', { sensitivity: 'base', numeric: true })`,
    ///   as used by `@ianvs/prettier-plugin-sort-imports`.
    ///   Symbols sort before digits and digits before letters (e.g. `@scope/pkg` before `#subpath`, `a_b` before `a-b`).
    ///   The whole module specifier is compared, including any `?query`.
    ///   Case is always ignored; with `ignoreCase: false`, names that compare equal are ordered by code point.
    ///   Non-ASCII characters sort after ASCII letters by code point, which can differ from `Intl.Collator`.
    ///
    /// - Default: `"natural"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collation: Option<SortCollationConfig>,
    // JHPATCH: sortImports.sortSpecifiers option to match @ianvs/prettier-plugin-sort-imports
    /// Sort named specifiers inside `{ ... }`.
    ///
    /// Value specifiers come before inline `type` specifiers, then specifiers are ordered by their local name,
    /// following `collation`, `ignoreCase` and `order`.
    /// Specifier lists that contain comments are left as written.
    ///
    /// ```js
    /// import { alpha, Beta, Zed, type Alpha } from 'x'
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_specifiers: Option<bool>,
    // JHPATCH: sortImports.hoist option to match @ianvs/prettier-plugin-sort-imports
    /// Move every top-level import into one block at the top of the file,
    /// the way `@ianvs/prettier-plugin-sort-imports` does.
    ///
    /// The block follows the hashbang, the directives (and a blank line), and the comments directly after them.
    /// A blank line separates the block from the rest of the file.
    /// An import takes along the comments between the previous statement and itself,
    /// except those on the line where the previous import starts, which stay with that import.
    /// Comments that end on a line before the first import stay at the top of the block when imports are sorted.
    /// A blank line separates an import's trailing comment from the next import's leading comment.
    /// `export ... from` statements are not moved, and imports inside `declare module` blocks are not sorted.
    ///
    /// ```js
    /// import { a } from 'a'
    /// // Used by `run`.
    /// import { b } from 'b'
    ///
    /// const x = 1
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    // JHPATCH: sortImports.mergeDuplicates option to match @ianvs/prettier-plugin-sort-imports
    /// Merge imports of the same source into one declaration,
    /// the way `@ianvs/prettier-plugin-sort-imports` does.
    ///
    /// Imports are merged within the imports that are sorted together
    /// (every top-level import with `hoist`, otherwise each run of imports, split by the `partitionBy*` options).
    /// The first import of a source takes in the later ones, with their specifiers after its own
    /// and the comments above them below its own comments.
    /// Value imports merge with value imports and type imports with type imports.
    /// With `combineTypeAndValueImports` and without `splitTypeSpecifiers`,
    /// type imports also merge with value imports as inline `type` specifiers.
    ///
    /// These are not merged: an import with a namespace specifier, two imports with a default specifier,
    /// a type import with a default specifier, side effect imports, suppressed imports,
    /// imports with different import attributes, and imports with comments outside `{ ... }`.
    /// An import that cannot merge still keeps later imports of its source from merging into an earlier one.
    ///
    /// ```js
    /// import React, { useState } from 'react'
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_duplicates: Option<bool>,
    // JHPATCH: sortImports.splitTypeSpecifiers option to match @ianvs/prettier-plugin-sort-imports
    /// Split the inline `type` specifiers of a value import into a type import,
    /// the way `@ianvs/prettier-plugin-sort-imports` does.
    ///
    /// Applies to a value import with more than one specifier, some but not all of them inline `type` specifiers.
    /// `import { type A } from 'a'` and `import { type A, type B } from 'a'` are left as written,
    /// as are imports with import attributes.
    /// Splitting runs after `mergeDuplicates`, so the new type import is not merged with other type imports
    /// of its source until the file is formatted again.
    ///
    /// With `hoist`, the comments of a split import move below the imports, as they do with Prettier.
    ///
    /// ```js
    /// import { b } from 'a'
    /// import type { A } from 'a'
    /// ```
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type_specifiers: Option<bool>,
    // JHPATCH: sortImports.combineTypeAndValueImports option to match @ianvs/prettier-plugin-sort-imports below TypeScript 4.5
    /// Let `mergeDuplicates` merge type imports and value imports of the same source,
    /// with the type import's named specifiers marked `type`.
    ///
    /// `@ianvs/prettier-plugin-sort-imports` turns this off when `importOrderTypeScriptVersion` is below 4.5.0.
    /// Has no effect with `splitTypeSpecifiers`.
    ///
    /// ```js
    /// // `true`
    /// import { a, type A } from 'a'
    /// // `false`
    /// import { a } from 'a'
    /// import type { A } from 'a'
    /// ```
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combine_type_and_value_imports: Option<bool>,
    /// Add newlines between groups.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newlines_between: Option<bool>,
    /// Prefixes for identifying internal imports.
    ///
    /// - Default: `["~/", "@/", "#"]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_pattern: Option<Vec<String>>,
    /// List of import groups for sorting.
    ///
    /// Each import is assigned to a single group (or `unknown` if none matches), and groups are ordered as listed.
    /// Within a group, imports are sorted according to `order`, `ignoreCase`, etc.
    ///
    /// A predefined group name is a single selector with optional modifiers, joined by `-` (e.g. `type-external`).
    /// Modifiers can be in any order, but the selector must come last.
    ///
    /// Selectors, from most to least important:
    /// - `type`: TypeScript type imports.
    /// - `side_effect_style`: Side effect style imports.
    /// - `side_effect`: Side effect imports.
    /// - `style`: Style imports.
    /// - `index`: Main file from the current directory.
    /// - `sibling`: Modules from the same directory.
    /// - `parent`: Modules from the parent directory.
    /// - `subpath`: Node.js subpath imports.
    /// - `internal`: Your internal modules.
    /// - `builtin`: Node.js Built-in Modules.
    /// - `external`: External modules installed in the project.
    /// - `import`: Any import.
    ///
    /// Modifiers, from most to least important:
    /// - `side_effect`: Side effect imports.
    /// - `type`: TypeScript type imports.
    /// - `value`: Value imports.
    /// - `default`: Imports containing the default specifier.
    /// - `wildcard`: Imports containing the wildcard (`* as`) specifier.
    /// - `named`: Imports containing at least one named specifier.
    ///
    /// Wrap groups in an array to sort them together as a single group (order within the array does not matter).
    ///
    /// To override `newlinesBetween` at a specific group boundary,
    /// put a `{ "newlinesBetween": boolean }` marker object at that position.
    ///
    /// - Default: See below
    /// ```json
    /// [
    ///   "builtin",
    ///   "external",
    ///   ["internal", "subpath"],
    ///   ["parent", "sibling", "index"],
    ///   "style",
    ///   "unknown"
    /// ]
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<SortGroupItemConfig>>,
    /// Define your own groups for matching very specific imports.
    ///
    /// The first matching definition is used, and custom groups take precedence over predefined groups.
    /// To give a predefined group precedence, define an equivalent custom group and put it first.
    ///
    /// When multiple conditions (`elementNamePattern`, `selector`, `modifiers`) are specified,
    /// all of them must match.
    ///
    /// NOTE: Predefined group names (e.g. `side_effect`, `external`) and `unknown` are reserved and cannot be used as `groupName`.
    ///
    /// - Default: `[]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_groups: Option<Vec<CustomGroupItemConfig>>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrderConfig {
    Asc,
    Desc,
}

// JHPATCH: sortImports.collation values
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortCollationConfig {
    Natural,
    Intl,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SortGroupItemConfig {
    /// A `{ "newlinesBetween": bool }` marker object that overrides the global `newlinesBetween`
    /// setting for the boundary between the previous and next groups.
    NewlinesBetween(NewlinesBetweenMarker),
    /// A single group name string (e.g. `"value-builtin"`).
    Single(String),
    /// Multiple group names treated as one group (e.g. `["value-builtin", "value-external"]`).
    Multiple(Vec<String>),
}

/// A marker object for overriding `newlinesBetween` at a specific group boundary.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewlinesBetweenMarker {
    pub newlines_between: bool,
}

impl SortGroupItemConfig {
    pub(super) fn into_vec(self) -> Vec<String> {
        match self {
            Self::Single(s) => vec![s],
            Self::Multiple(v) => v,
            Self::NewlinesBetween(_) => {
                unreachable!("NewlinesBetween markers should be handled before calling into_vec")
            }
        }
    }

    /// Validate the flat `groups` list's marker grammar: a `{ "newlinesBetween" }`
    /// marker must sit BETWEEN two groups, so it may not lead, trail, or neighbor another marker.
    ///
    /// These are rules of this syntax (the parallel-vec target cannot even represent a violation),
    /// so their owner is this type, mirroring `SortImportsOptions::validate` on the formatter side.
    ///
    /// # Errors
    /// Returns an error naming the violated position rule.
    pub(super) fn validate_markers(items: &[Self]) -> Result<(), String> {
        let is_marker = |item: &Self| matches!(item, Self::NewlinesBetween(_));
        if items.first().is_some_and(is_marker) {
            return Err("`{ \"newlinesBetween\" }` marker cannot appear at the start of `groups`"
                .to_string());
        }
        if items.last().is_some_and(is_marker) {
            return Err(
                "`{ \"newlinesBetween\" }` marker cannot appear at the end of `groups`".to_string()
            );
        }
        if items.windows(2).any(|pair| is_marker(&pair[0]) && is_marker(&pair[1])) {
            return Err(
                "consecutive `{ \"newlinesBetween\" }` markers are not allowed in `groups`"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomGroupItemConfig {
    /// Name of the custom group, used in the `groups` option.
    pub group_name: String,
    /// List of glob patterns to match import sources for this group.
    pub element_name_pattern: Vec<String>,
    /// Selector to match the import kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<ImportSelectorConfig>,
    /// Modifiers to match the import characteristics.
    /// All specified modifiers must match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<ImportModifierConfig>>,
}

/// Selector matching the import kind in `customGroups` (see `sortImports.groups` for semantics).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportSelectorConfig {
    Type,
    SideEffectStyle,
    SideEffect,
    Style,
    Index,
    Sibling,
    Parent,
    Subpath,
    Internal,
    Builtin,
    External,
    Import,
}

/// Modifier matching the import characteristics in `customGroups` (see `sortImports.groups` for semantics).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportModifierConfig {
    SideEffect,
    Type,
    Value,
    Default,
    Wildcard,
    Named,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SortPackageJsonUserConfig {
    Bool(bool),
    Object(SortPackageJsonConfig),
}

impl Default for SortPackageJsonUserConfig {
    fn default() -> Self {
        Self::Bool(true)
    }
}

impl SortPackageJsonUserConfig {
    pub fn into_config(self) -> Option<SortPackageJsonConfig> {
        match self {
            Self::Bool(false) => None,
            Self::Bool(true) => Some(SortPackageJsonConfig::default()),
            Self::Object(config) => Some(config),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct SortPackageJsonConfig {
    /// Sort the `scripts` field alphabetically.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_scripts: Option<bool>,
    // JHPATCH: sortPackageJson.preset option for prettier-plugin-packagejson's order
    /// The field order and nested sorting rules.
    ///
    /// - `"oxfmt"`: Oxfmt's own order.
    /// - `"sort-package-json"`: The order of the [sort-package-json](https://github.com/keithamus/sort-package-json) npm package 3.5.0,
    ///   which prettier-plugin-packagejson uses. `sortScripts` has no effect:
    ///   `scripts` are sorted with `pre<name>` and `post<name>` next to `<name>`,
    ///   unless an `npm-run-all` script runs a glob sequentially.
    ///
    /// - Default: `"oxfmt"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<SortPackageJsonPresetConfig>,
}

// JHPATCH: sortPackageJson.preset values
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SortPackageJsonPresetConfig {
    Oxfmt,
    SortPackageJson,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SortTailwindcssUserConfig {
    Bool(bool),
    Object(SortTailwindcssConfig),
}

impl SortTailwindcssUserConfig {
    pub fn into_config(self) -> Option<SortTailwindcssConfig> {
        match self {
            Self::Bool(true) => Some(SortTailwindcssConfig::default()),
            Self::Bool(false) => None,
            Self::Object(config) => Some(config),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct SortTailwindcssConfig {
    /// Path to your Tailwind CSS configuration file (v3).
    ///
    /// NOTE: Paths are resolved relative to the Oxfmt configuration file.
    ///
    /// - Default: Automatically find `"tailwind.config.js"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// Path to your Tailwind CSS stylesheet (v4).
    ///
    /// NOTE: Paths are resolved relative to the Oxfmt configuration file.
    ///
    /// - Default: Installed Tailwind CSS's `theme.css`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylesheet: Option<String>,

    /// List of custom function names whose arguments should be sorted (exact match).
    ///
    /// NOTE: Regex patterns are not yet supported.
    ///
    /// - Default: `[]`
    /// - Example: `["clsx", "cn", "cva", "tw"]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<String>>,
    /// List of additional attributes to sort beyond `class` and `className` (exact match).
    ///
    /// NOTE: Regex patterns are not yet supported.
    ///
    /// - Default: `[]`
    /// - Example: `["myClassProp", ":class"]`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<String>>,

    /// Preserve whitespace around classes.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_whitespace: Option<bool>,
    /// Preserve duplicate classes.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_duplicates: Option<bool>,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum JsdocUserConfig {
    Bool(bool),
    Object(JsdocConfig),
}

impl JsdocUserConfig {
    pub fn into_config(self) -> Option<JsdocConfig> {
        match self {
            Self::Bool(true) => Some(JsdocConfig::default()),
            Self::Bool(false) => None,
            Self::Object(config) => Some(config),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JsdocConfig {
    /// Capitalize the first letter of tag descriptions.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capitalize_descriptions: Option<bool>,
    /// Add a trailing dot to the end of descriptions.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_with_dot: Option<bool>,
    /// Append default values to `@param` descriptions (e.g. "Default is `value`").
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_default_to_description: Option<bool>,
    /// Use fenced code blocks (```` ``` ````) instead of 4-space indentation for code without a language tag.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefer_code_fences: Option<bool>,
    /// Strategy for wrapping description lines at print width.
    ///
    /// - `"greedy"`: Always re-wrap text to fit within print width
    /// - `"balance"`: Preserve original line breaks if all lines fit within print width
    ///
    /// By default, description lines are always re-wrapped.
    ///
    /// - Default: `"greedy"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_wrapping_style: Option<LineWrappingStyleConfig>,
    /// How to format comment blocks.
    ///
    /// - `"singleLine"`: Convert to single-line `/** content */` when possible
    /// - `"multiline"`: Always use multi-line format
    /// - `"keep"`: Preserve original formatting
    ///
    /// By default, comments are collapsed to a single line when possible.
    ///
    /// - Default: `"singleLine"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_line_strategy: Option<CommentLineStrategyConfig>,
    /// Add blank lines between different tag groups (e.g. between `@param` and `@returns`).
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separate_tag_groups: Option<bool>,
    /// Add a blank line between the last `@param` and `@returns`.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separate_returns_from_param: Option<bool>,
    /// Add spaces inside JSDoc type braces: `{string}` → `{ string }`.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bracket_spacing: Option<bool>,
    /// Emit `@description` tag instead of inline description.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_tag: Option<bool>,
    /// Preserve indentation in unparsable `@example` code.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_unparsable_example_indent: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineWrappingStyleConfig {
    Greedy,
    Balance,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CommentLineStrategyConfig {
    SingleLine,
    Multiline,
    Keep,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SvelteUserConfig {
    Bool(bool),
    Object(SvelteConfig),
}

impl SvelteUserConfig {
    pub fn into_config(self) -> Option<SvelteConfig> {
        match self {
            Self::Bool(true) => Some(SvelteConfig::default()),
            Self::Bool(false) => None,
            Self::Object(config) => Some(config),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct SvelteConfig {
    /// The order in which Svelte component sections are printed.
    /// Join `options`, `scripts`, `markup`, `styles` with `-` in the desired order,
    /// or use `none` to keep the original order.
    ///
    /// - Default: `"options-scripts-markup-styles"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<String>,
    /// Whether to allow attribute shorthand if attribute name and expression are the same.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_shorthand: Option<bool>,
    /// Whether to indent code inside `<script>` and `<style>` tags.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_script_and_style: Option<bool>,
}

// ---

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum AstroUserConfig {
    Bool(bool),
    Object(AstroConfig),
}

impl AstroUserConfig {
    pub fn into_config(self) -> Option<AstroConfig> {
        match self {
            Self::Bool(true) => Some(AstroConfig::default()),
            Self::Bool(false) => None,
            Self::Object(config) => Some(config),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct AstroConfig {
    /// Whether to normalize matching identifier attributes to shorthand or explicit form.
    /// When unset, the form that was written stays as-is.
    ///
    /// - Default: Unset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_shorthand: Option<bool>,
    /// Whether to skip formatting the frontmatter.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_frontmatter: Option<bool>,
    /// Mirror of Astro's `compressHTML` config.
    /// Tells the formatter which whitespace the compiler will collapse.
    ///
    /// - Default: `"jsx"`
    #[serde(rename = "compressHTML", skip_serializing_if = "Option::is_none")]
    pub compress_html: Option<AstroCompressHtmlConfig>,
    // JHPATCH: astro.useProjectPlugin option for projects that pin prettier-plugin-astro
    /// Whether to use the `prettier-plugin-astro` installed in your project (e.g. `0.14.x`) instead of the bundled version.
    /// It is resolved from the directory of the config file (the current working directory for the Node.js API).
    /// Formatting fails if it cannot be resolved.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_project_plugin: Option<bool>,
    /// Directory `prettier-plugin-astro` is resolved from when `use_project_plugin` is enabled.
    #[serde(skip)]
    #[schemars(skip)]
    pub project_plugin_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AstroCompressHtmlConfig {
    /// Whitespace runs containing a newline are dropped; same-line spaces are content.
    Jsx,
    /// Browser HTML whitespace rules.
    Html,
    /// No collapsing.
    None,
}

// ---

// JHPATCH: markdown options (markdown.lazyCodeSpanContinuation)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct MarkdownConfig {
    /// Print the continuation lines of a multi-line code span from column 0,
    /// without the indentation or `>` markers of the enclosing list items and blockquotes
    /// (lazy continuation lines), as Prettier does.
    ///
    /// Applies only when no continuation line starts with whitespace, so the code span reads the same.
    /// Otherwise, and when disabled, continuation lines resume at the container's content column.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lazy_code_span_continuation: Option<bool>,
}

// ---

// JHPATCH: toml options: oxc-toml settings under prettier-plugin-toml names
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct TomlConfig {
    /// Align consecutive entries vertically.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_entries: Option<bool>,
    /// Align consecutive comments after entries and array items vertically.
    ///
    /// - Default: `false` (`prettier-plugin-toml`: `true`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_comments: Option<bool>,
    /// When `alignComments` is enabled, also align a comment that has no neighbor.
    ///
    /// `prettier-plugin-toml` has no such option and always aligns it.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_single_comments: Option<bool>,
    /// The maximum number of consecutive blank lines.
    ///
    /// - Default: `2` (`prettier-plugin-toml`: `1`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_blank_lines: Option<u32>,
    /// Expand arrays that exceed `printWidth` to multiple lines.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub array_auto_expand: Option<bool>,
    /// Collapse arrays that fit in `printWidth` and contain no comments.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub array_auto_collapse: Option<bool>,
    /// Omit whitespace padding inside single-line arrays.
    ///
    /// - Default: `true`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_arrays: Option<bool>,
    /// Omit whitespace padding inside inline tables.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_inline_tables: Option<bool>,
    /// Omit whitespace around `=`.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_entries: Option<bool>,
    /// Indent subtables and arrays of tables under their parent table, if they come in order.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_tables: Option<bool>,
    /// Indent entries under tables.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_entries: Option<bool>,
    /// Alphabetically reorder keys that are not separated by blank lines.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reorder_keys: Option<bool>,
    // JHPATCH: toml.trimLeadingBlankLines, which prettier-plugin-toml always does
    /// Remove the blank lines at the start of the file.
    ///
    /// `prettier-plugin-toml` has no such option and always removes them.
    ///
    /// - Default: `false`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_leading_blank_lines: Option<bool>,
}

// ---

/// Merge two JSON values recursively.
/// - Overlay values overwrite base values (any non-object overlay wins, including `null`)
///
/// All Prettier options are flat, but some our options are nested.
///
/// NOTE: null-as-reset is helper-level only.
/// [`FormatConfig::merge`] re-serializes typed `Option<T>` fields with `skip_serializing_if = "Option::is_none"`,
/// so `None` is omitted (not emitted as `null`) and never reaches this function.
/// User-facing override `null` therefore has no effect, explicit field omission has the same result.
fn json_deep_merge(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                let merged = if let Some(base_value) = base_map.remove(&key) {
                    json_deep_merge(base_value, overlay_value)
                } else {
                    overlay_value
                };
                base_map.insert(key, merged);
            }
            Value::Object(base_map)
        }
        (_base, overlay) => overlay,
    }
}

// ---

#[cfg(test)]
mod tests_json_deep_merge {
    use super::*;

    #[test]
    fn test_json_deep_merge() {
        use serde_json::json;

        // Primitives: overlay wins
        let base = json!({ "semi": true, "tabWidth": 2 });
        let overlay = json!({ "semi": false });
        let merged = json_deep_merge(base, overlay);
        assert_eq!(merged, json!({ "semi": false, "tabWidth": 2 }));

        // Nested objects: deep merge
        let base = json!({ "experimentalSortImports": { "order": "asc", "ignoreCase": true } });
        let overlay = json!({ "experimentalSortImports": { "order": "desc" } });
        let merged = json_deep_merge(base, overlay);
        assert_eq!(
            merged,
            json!({ "experimentalSortImports": { "order": "desc", "ignoreCase": true } })
        );
    }
}

// ---

#[cfg(test)]
mod tests_reject_experimental {
    use super::*;

    #[test]
    fn test_reject_experimental_ternaries_in_base() {
        let json = r#"{ "experimentalTernaries": true }"#;
        let err = serde_json::from_str::<FormatConfig>(json).unwrap_err();
        assert!(err.to_string().contains("experimentalTernaries"));
    }

    #[test]
    fn test_reject_experimental_in_overrides() {
        // `OxfmtOverrideConfig.options: FormatConfig` so the same deserialize_with applies
        let json = r#"{
            "overrides": [
                {
                    "files": ["*.ts"],
                    "options": { "experimentalTernaries": true }
                }
            ]
        }"#;
        let err = serde_json::from_str::<Oxfmtrc>(json).unwrap_err();
        assert!(err.to_string().contains("experimentalTernaries"));
    }
}
