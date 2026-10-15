// JHPATCH: sort-package-json npm package order (sortPackageJson.preset: "sort-package-json")
//! The field order and nested sorting of the npm package
//! [`sort-package-json`](https://github.com/keithamus/sort-package-json) 3.5.0
//! (<https://github.com/keithamus/sort-package-json/blob/v3.5.0/index.js>).
//!
//! Key comparisons follow JavaScript semantics:
//! - `Array.prototype.sort()` without a comparator compares UTF-16 code units ([`cmp_utf16`]).
//! - `String.prototype.localeCompare(other, 'en')` uses ICU collation ([`locale_compare`]).
//! - Object keys that are array indices (`"0"`, `"1"`, ...) always come first in ascending
//!   numeric order ([`hoist_array_index_keys`]).

use std::cmp::Ordering;

use crate::value::{Object, Value};

const FIELDS: &[&str] = &[
    "$schema",
    "name",
    "displayName",
    "version",
    "stableVersion",
    "private",
    "description",
    "categories",
    "keywords",
    "homepage",
    "bugs",
    "repository",
    "funding",
    "license",
    "qna",
    "author",
    "maintainers",
    "contributors",
    "publisher",
    "sideEffects",
    "type",
    "imports",
    "exports",
    "main",
    "svelte",
    "umd:main",
    "jsdelivr",
    "unpkg",
    "module",
    "source",
    "jsnext:main",
    "browser",
    "react-native",
    "types",
    "typesVersions",
    "typings",
    "style",
    "example",
    "examplestyle",
    "assets",
    "bin",
    "man",
    "directories",
    "files",
    "workspaces",
    "binary",
    "scripts",
    "betterScripts",
    "l10n",
    "contributes",
    "activationEvents",
    "husky",
    "simple-git-hooks",
    "pre-commit",
    "commitlint",
    "lint-staged",
    "nano-staged",
    "config",
    "nodemonConfig",
    "browserify",
    "babel",
    "browserslist",
    "xo",
    "prettier",
    "eslintConfig",
    "eslintIgnore",
    "npmpkgjsonlint",
    "npmPackageJsonLintConfig",
    "npmpackagejsonlint",
    "release",
    "remarkConfig",
    "stylelint",
    "ava",
    "jest",
    "jest-junit",
    "jest-stare",
    "mocha",
    "nyc",
    "c8",
    "tap",
    "oclif",
    "resolutions",
    "overrides",
    "dependencies",
    "devDependencies",
    "dependenciesMeta",
    "peerDependencies",
    "peerDependenciesMeta",
    "optionalDependencies",
    "bundledDependencies",
    "bundleDependencies",
    "extensionPack",
    "extensionDependencies",
    "flat",
    "packageManager",
    "engines",
    "engineStrict",
    "volta",
    "languageName",
    "os",
    "cpu",
    "preferGlobal",
    "publishConfig",
    "icon",
    "badges",
    "galleryBanner",
    "preview",
    "markdown",
    "pnpm",
];

/// The `git-hooks-list` 4.1.1 package.
const GIT_HOOKS: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];

const ESLINT_BASE_CONFIG_PROPERTIES: &[&str] = &[
    "files",
    "excludedFiles",
    "env",
    "parser",
    "parserOptions",
    "settings",
    "plugins",
    "extends",
    "rules",
    "overrides",
    "globals",
    "processor",
    "noInlineConfig",
    "reportUnusedDisableDirectives",
];

const PNPM_BASE_CONFIG_PROPERTIES: &[&str] = &[
    "peerDependencyRules",
    "neverBuiltDependencies",
    "onlyBuiltDependencies",
    "onlyBuiltDependenciesFile",
    "allowedDeprecatedVersions",
    "allowNonAppliedPatches",
    "updateConfig",
    "auditConfig",
    "requiredScripts",
    "supportedArchitectures",
    "overrides",
    "patchedDependencies",
    "packageExtensions",
];

const DEFAULT_NPM_SCRIPTS: &[&str] = &[
    "install",
    "pack",
    "prepare",
    "publish",
    "restart",
    "shrinkwrap",
    "start",
    "stop",
    "test",
    "uninstall",
    "version",
];

const PEOPLE_KEYS: &[&str] = &["name", "email", "url"];
const URL_KEYS: &[&str] = &["type", "url"];

pub(crate) fn sort_package_json(mut obj: Object<'_>) -> Object<'_> {
    hoist_array_index_keys_deep(&mut obj);
    let sequential = has_sequential_script(&obj);

    obj.sort_by(|(a, _), (b, _)| {
        let a_index = FIELDS.iter().position(|field| field == a);
        let b_index = FIELDS.iter().position(|field| field == b);
        match (a_index, b_index) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => {
                a.starts_with('_').cmp(&b.starts_with('_')).then_with(|| cmp_utf16(a, b))
            }
        }
    });

    let mut obj: Object<'_> = obj
        .into_iter()
        .map(|(key, value)| {
            let value = sort_field(&key, value, sequential);
            (key, value)
        })
        .collect();
    hoist_array_index_keys_deep(&mut obj);
    obj
}

fn sort_field<'a>(key: &str, value: Value<'a>, sequential: bool) -> Value<'a> {
    match key {
        "categories" | "keywords" | "files" | "activationEvents" => uniq(value),
        "bugs" => on_object(value, |o| sort_by_key_order(o, &["url", "email"])),
        "repository" | "funding" | "license" => {
            on_object(value, |o| sort_by_key_order(o, URL_KEYS))
        }
        "author" => on_object(value, |o| sort_by_key_order(o, PEOPLE_KEYS)),
        "maintainers" | "contributors" => on_array(value, |arr| {
            arr.into_iter().map(|v| on_object(v, |o| sort_by_key_order(o, PEOPLE_KEYS))).collect()
        }),
        "exports" => sort_exports(value),
        "directories" => on_object(value, |o| {
            sort_by_key_order(o, &["lib", "bin", "man", "doc", "example", "test"])
        }),
        "workspaces" => on_object(value, sort_workspaces),
        "binary" => on_object(value, |o| {
            sort_by_key_order(
                o,
                &["module_name", "module_path", "remote_path", "package_name", "host"],
            )
        }),
        "scripts" | "betterScripts" => on_object(value, |o| sort_scripts(o, sequential)),
        "husky" => on_object(value, |o| {
            map_property(o, "hooks", |v| on_object(v, |o| sort_by_key_order(o, GIT_HOOKS)))
        }),
        "simple-git-hooks" => on_object(value, |o| sort_by_key_order(o, GIT_HOOKS)),
        "bin"
        | "contributes"
        | "commitlint"
        | "config"
        | "nodemonConfig"
        | "browserify"
        | "babel"
        | "xo"
        | "npmpkgjsonlint"
        | "npmPackageJsonLintConfig"
        | "npmpackagejsonlint"
        | "release"
        | "remarkConfig"
        | "ava"
        | "jest"
        | "jest-junit"
        | "jest-stare"
        | "mocha"
        | "nyc"
        | "c8"
        | "tap"
        | "resolutions"
        | "engines"
        | "engineStrict"
        | "preferGlobal"
        | "publishConfig"
        | "galleryBanner" => on_object(value, sort_alphabetically),
        "prettier" => on_object(value, sort_prettier_config),
        "eslintConfig" => sort_eslint_config(value),
        "oclif" => on_object(value, |o| sort_deep(o, &sort_alphabetically)),
        "overrides"
        | "dependencies"
        | "devDependencies"
        | "peerDependencies"
        | "optionalDependencies" => on_object(value, sort_dependencies),
        "dependenciesMeta" => on_object(value, |o| {
            sort_deep(o, &|o| sort_by(o, |a, b| cmp_utf16(package_name(a), package_name(b))))
        }),
        "peerDependenciesMeta" => on_object(value, |o| sort_deep(o, &sort_alphabetically)),
        "bundledDependencies"
        | "bundleDependencies"
        | "extensionPack"
        | "extensionDependencies" => uniq_and_sort(value),
        "volta" => on_object(value, |o| sort_by_key_order(o, &["node", "npm", "yarn"])),
        "badges" => on_array(value, |arr| {
            arr.into_iter()
                .map(|v| on_object(v, |o| sort_by_key_order(o, &["description", "url", "href"])))
                .collect()
        }),
        "pnpm" => on_object(value, sort_pnpm_config),
        _ => value,
    }
}

// ===== Generic helpers ======================================================

fn on_object<'a>(value: Value<'a>, f: impl FnOnce(Object<'a>) -> Object<'a>) -> Value<'a> {
    match value {
        Value::Object(o) => Value::Object(f(o)),
        other => other,
    }
}

fn on_array<'a>(value: Value<'a>, f: impl FnOnce(Vec<Value<'a>>) -> Vec<Value<'a>>) -> Value<'a> {
    match value {
        Value::Array(arr) => Value::Array(f(arr)),
        other => other,
    }
}

/// Applies `f` to the value of `key` if `obj` has it.
fn map_property<'a>(
    mut obj: Object<'a>,
    key: &str,
    f: impl FnOnce(Value<'a>) -> Value<'a>,
) -> Object<'a> {
    if let Some(entry) = obj.iter_mut().find(|(k, _)| k == key) {
        let value = std::mem::replace(&mut entry.1, Value::Null);
        entry.1 = f(value);
    }
    obj
}

/// Removes duplicates from an array whose items are all strings, keeping the first occurrence.
fn uniq(value: Value<'_>) -> Value<'_> {
    on_array(value, |mut arr| {
        if !arr.iter().all(Value::is_string) {
            return arr;
        }
        let mut write = 0;
        for read in 0..arr.len() {
            let is_duplicate = arr[..write].iter().any(|seen| seen.as_str() == arr[read].as_str());
            if !is_duplicate {
                arr.swap(write, read);
                write += 1;
            }
        }
        arr.truncate(write);
        arr
    })
}

/// [`uniq`], then sorts an array whose items are all strings.
fn uniq_and_sort(value: Value<'_>) -> Value<'_> {
    on_array(uniq(value), |mut arr| {
        if arr.iter().all(Value::is_string) {
            arr.sort_by(|a, b| cmp_utf16(a.as_str().unwrap_or(""), b.as_str().unwrap_or("")));
        }
        arr
    })
}

/// Stable sort of the keys.
fn sort_by<'a>(mut obj: Object<'a>, cmp: impl Fn(&str, &str) -> Ordering) -> Object<'a> {
    obj.sort_by(|(a, _), (b, _)| cmp(a, b));
    obj
}

fn sort_alphabetically(obj: Object<'_>) -> Object<'_> {
    sort_by(obj, cmp_utf16)
}

/// Keys in `order` first (by first occurrence in `order`), then the rest alphabetically.
fn sort_by_key_order<'a, S: AsRef<str>>(obj: Object<'a>, order: &[S]) -> Object<'a> {
    let position = |key: &str| order.iter().position(|k| k.as_ref() == key);
    sort_by(obj, |a, b| match (position(a), position(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => cmp_utf16(a, b),
    })
}

/// Applies `sort` to `obj` and, first, to every object nested in its values (not inside arrays).
fn sort_deep<'a, F>(obj: Object<'a>, sort: &F) -> Object<'a>
where
    F: Fn(Object<'a>) -> Object<'a>,
{
    let obj = obj
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::Object(o) => Value::Object(sort_deep(o, sort)),
                other => other,
            };
            (key, value)
        })
        .collect();
    sort(obj)
}

/// `sortDependenciesLikeNpm`: `a.localeCompare(b, 'en')`.
fn sort_dependencies(obj: Object<'_>) -> Object<'_> {
    sort_by(obj, locale_compare)
}

fn is_truthy(value: &Value<'_>) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

// ===== Field-specific sorters ===============================================

/// `exports`: paths (`.`-prefixed keys) keep their order, followed by conditions:
/// `types` and `types@*` first, `default` last, the rest in their original order.
fn sort_exports(value: Value<'_>) -> Value<'_> {
    on_object(value, |obj| {
        let rank = |key: &str| {
            if key.starts_with('.') {
                0
            } else if key == "types" || key.starts_with("types@") {
                1
            } else if key == "default" {
                3
            } else {
                2
            }
        };
        let mut obj: Object<'_> =
            obj.into_iter().map(|(key, value)| (key, sort_exports(value))).collect();
        obj.sort_by_key(|(key, _)| rank(key));
        obj
    })
}

/// `workspaces` object form: `packages` (deduplicated and sorted), `catalog` (sorted like
/// dependencies), then the other keys alphabetically.
fn sort_workspaces(obj: Object<'_>) -> Object<'_> {
    let mut packages = None;
    let mut catalog = None;
    let mut others = Vec::with_capacity(obj.len());
    for (key, value) in obj {
        match key.as_ref() {
            "packages" if is_truthy(&value) => packages = Some((key, uniq_and_sort(value))),
            "catalog" if is_truthy(&value) => {
                catalog = Some((key, on_object(value, sort_dependencies)));
            }
            _ => others.push((key, value)),
        }
    }
    others.sort_by(|(a, _), (b, _)| cmp_utf16(a, b));
    packages.into_iter().chain(catalog).chain(others).collect()
}

/// Sorts script names alphabetically, keeping `pre<name>` and `post<name>` around `<name>` when
/// `<name>` is a script or an npm lifecycle script. Names keep their order when `sequential`.
fn sort_scripts(obj: Object<'_>, sequential: bool) -> Object<'_> {
    let names: Vec<&str> = obj.iter().map(|(key, _)| key.as_ref()).collect();
    let mut prefixable: Vec<&str> = Vec::new();
    let mut keys: Vec<&str> = names
        .iter()
        .map(|&name| {
            let omitted =
                name.strip_prefix("pre").or_else(|| name.strip_prefix("post")).unwrap_or(name);
            if DEFAULT_NPM_SCRIPTS.contains(&omitted) || names.contains(&omitted) {
                if !prefixable.contains(&omitted) {
                    prefixable.push(omitted);
                }
                omitted
            } else {
                name
            }
        })
        .collect();
    if !sequential {
        keys.sort_by(|a, b| cmp_utf16(a, b));
    }
    let order: Vec<String> = keys
        .iter()
        .flat_map(|&key| {
            if prefixable.contains(&key) {
                vec![format!("pre{key}"), key.to_string(), format!("post{key}")]
            } else {
                vec![key.to_string()]
            }
        })
        .collect();
    sort_by_key_order(obj, &order)
}

/// Whether `npm-run-all` / `npm-run-all2` is a dev dependency and a script runs a glob
/// sequentially, in which case script order is significant.
fn has_sequential_script(obj: &Object<'_>) -> bool {
    let has_npm_run_all = obj.iter().any(|(key, value)| {
        key == "devDependencies"
            && matches!(value, Value::Object(deps) if deps
                .iter()
                .any(|(name, _)| name == "npm-run-all" || name == "npm-run-all2"))
    });
    if !has_npm_run_all {
        return false;
    }
    obj.iter()
        .filter(|(key, _)| key == "scripts" || key == "betterScripts")
        .flat_map(|(_, value)| match value {
            Value::Object(scripts) => scripts.iter().map(|(_, v)| v).collect::<Vec<_>>(),
            Value::Array(scripts) => scripts.iter().collect(),
            _ => Vec::new(),
        })
        .filter_map(Value::as_str)
        .any(is_sequential_command)
}

/// `command.includes('*')` and
/// `/(?<=^|[\s&;<>|(])(?:run-s|npm-run-all2? .*(?:--sequential|--serial|-s))(?=$|[\s&;<>|)])/`.
fn is_sequential_command(command: &str) -> bool {
    fn is_js_whitespace(c: char) -> bool {
        (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
    }
    fn is_line_terminator(c: char) -> bool {
        matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
    }
    let ends_at_boundary = |end: usize| {
        command[end..]
            .chars()
            .next()
            .map_or(true, |c| is_js_whitespace(c) || matches!(c, '&' | ';' | '<' | '>' | '|' | ')'))
    };

    if !command.contains('*') {
        return false;
    }
    command.char_indices().any(|(start, _)| {
        let starts_at_boundary = command[..start].chars().next_back().map_or(true, |c| {
            is_js_whitespace(c) || matches!(c, '&' | ';' | '<' | '>' | '|' | '(')
        });
        if !starts_at_boundary {
            return false;
        }
        let rest = &command[start..];
        if rest.starts_with("run-s") && ends_at_boundary(start + "run-s".len()) {
            return true;
        }
        let Some(after) = rest.strip_prefix("npm-run-all") else {
            return false;
        };
        let after = after.strip_prefix('2').unwrap_or(after);
        let Some(args) = after.strip_prefix(' ') else {
            return false;
        };
        let args_start = command.len() - args.len();
        let line_end = args.find(is_line_terminator).unwrap_or(args.len());
        args[..line_end].char_indices().any(|(offset, _)| {
            let flag_start = args_start + offset;
            ["--sequential", "--serial", "-s"].iter().any(|flag| {
                command[flag_start..].starts_with(flag) && ends_at_boundary(flag_start + flag.len())
            })
        })
    })
}

/// Keys alphabetically with `overrides` last; each `overrides[]` item and its `options`
/// sorted alphabetically.
fn sort_prettier_config(obj: Object<'_>) -> Object<'_> {
    let obj = sort_by(obj, |a, b| {
        (a == "overrides").cmp(&(b == "overrides")).then_with(|| cmp_utf16(a, b))
    });
    map_property(obj, "overrides", |value| {
        on_array(value, |overrides| {
            overrides
                .into_iter()
                .map(|item| {
                    on_object(item, |o| {
                        map_property(sort_alphabetically(o), "options", |options| {
                            on_object(options, sort_alphabetically)
                        })
                    })
                })
                .collect()
        })
    })
}

fn sort_eslint_config(value: Value<'_>) -> Value<'_> {
    on_object(value, |obj| {
        let obj = sort_by_key_order(obj, ESLINT_BASE_CONFIG_PROPERTIES);
        let obj = map_property(obj, "env", |v| on_object(v, sort_alphabetically));
        let obj = map_property(obj, "globals", |v| on_object(v, sort_alphabetically));
        let obj = map_property(obj, "overrides", |v| {
            on_array(v, |overrides| overrides.into_iter().map(sort_eslint_config).collect())
        });
        let obj = map_property(obj, "parserOptions", |v| on_object(v, sort_alphabetically));
        let obj = map_property(obj, "rules", |v| {
            on_object(v, |rules| {
                sort_by(rules, |a, b| {
                    a.matches('/')
                        .count()
                        .cmp(&b.matches('/').count())
                        .then_with(|| locale_compare(a, b))
                })
            })
        });
        map_property(obj, "settings", |v| on_object(v, sort_alphabetically))
    })
}

fn sort_pnpm_config(obj: Object<'_>) -> Object<'_> {
    let obj = sort_deep(obj, &|o| sort_by_key_order(o, PNPM_BASE_CONFIG_PROPERTIES));
    map_property(obj, "overrides", |v| on_object(v, |o| sort_by(o, compare_override_selectors)))
}

/// `getPackageName`: `@scope/name@1` -> `@scope/name`.
fn package_name(ident: &str) -> &str {
    let from = usize::from(ident.starts_with('@'));
    ident[from..].find('@').map_or(ident, |index| &ident[..from + index])
}

/// Compares pnpm override selectors such as `foo@^1>bar` by package name, then by the minimum
/// version of the version range.
fn compare_override_selectors(a: &str, b: &str) -> Ordering {
    /// `parseNameAndVersionRange`: the part before `>` split at its last `@` (not at index 0).
    fn parse(specifier: &str) -> (&str, Option<&str>) {
        let name_and_version = specifier.split('>').next().unwrap_or(specifier);
        match name_and_version.rfind('@') {
            Some(index) if index > 0 => {
                (&name_and_version[..index], Some(&name_and_version[index + 1..]))
            }
            _ => (specifier, None),
        }
    }

    let (a_name, a_range) = parse(a);
    let (b_name, b_range) = parse(b);
    if a_name != b_name {
        return locale_compare(a_name, b_name);
    }
    let Some(a_range) = a_range.filter(|range| !range.is_empty()) else {
        return Ordering::Less;
    };
    let Some(b_range) = b_range.filter(|range| !range.is_empty()) else {
        return Ordering::Greater;
    };
    match (semver::min_version(a_range), semver::min_version(b_range)) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => Ordering::Equal,
    }
}

// ===== JavaScript semantics =================================================

/// Compares strings by UTF-16 code units, like `Array.prototype.sort()` without a comparator.
fn cmp_utf16(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        return a.cmp(b);
    }
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `a.localeCompare(b, 'en')` (ICU root collation, tertiary strength) for ASCII strings.
/// Non-ASCII characters sort after ASCII by code point and have no case variants.
fn locale_compare(a: &str, b: &str) -> Ordering {
    /// The ASCII characters ICU does not ignore, in primary-weight order; a letter and its
    /// uppercase form share a weight.
    const ASCII_ORDER: &[u8] =
        b"\t\n\x0b\x0c\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";

    /// `(primary, is_uppercase)`, or `None` for characters ICU ignores (ASCII control characters
    /// other than `\t`, `\n`, `\v`, `\f`, `\r`).
    fn weights(c: char) -> Option<(u32, bool)> {
        if !c.is_ascii() {
            return Some((ASCII_ORDER.len() as u32 + c as u32, false));
        }
        let lower = c.to_ascii_lowercase() as u8;
        ASCII_ORDER
            .iter()
            .position(|&b| b == lower)
            .map(|primary| (primary as u32, c.is_ascii_uppercase()))
    }

    let a_weights = a.chars().filter_map(weights);
    let b_weights = b.chars().filter_map(weights);
    a_weights
        .clone()
        .map(|(primary, _)| primary)
        .cmp(b_weights.clone().map(|(primary, _)| primary))
        .then_with(|| a_weights.map(|(_, upper)| upper).cmp(b_weights.map(|(_, upper)| upper)))
}

/// Moves keys that are array indices to the front in ascending numeric order, recursively,
/// matching the property order of JavaScript objects.
fn hoist_array_index_keys_deep(obj: &mut Object<'_>) {
    for (_, value) in obj.iter_mut() {
        hoist_in_value(value);
    }
    hoist_array_index_keys(obj);
}

fn hoist_in_value(value: &mut Value<'_>) {
    match value {
        Value::Object(obj) => hoist_array_index_keys_deep(obj),
        Value::Array(arr) => arr.iter_mut().for_each(hoist_in_value),
        _ => {}
    }
}

fn hoist_array_index_keys(obj: &mut Object<'_>) {
    /// A canonical integer string in `0..2^32 - 1`.
    fn array_index(key: &str) -> Option<u32> {
        if key.is_empty()
            || key.len() > 10
            || (key.len() > 1 && key.starts_with('0'))
            || !key.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        key.parse::<u32>().ok().filter(|&index| index != u32::MAX)
    }

    if obj.iter().any(|(key, _)| array_index(key).is_some()) {
        obj.sort_by_key(|(key, _)| array_index(key).map_or((1, 0), |index| (0, index)));
    }
}

mod semver {
    //! `semver.minVersion` from the `semver` npm package for the range syntax pnpm overrides use.

    #[derive(Debug, PartialEq, Eq)]
    pub struct Version<'a> {
        major: u64,
        minor: u64,
        patch: u64,
        prerelease: Vec<&'a str>,
    }

    impl Ord for Version<'_> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            (self.major, self.minor, self.patch)
                .cmp(&(other.major, other.minor, other.patch))
                .then_with(|| match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (false, false) => {
                        for (a, b) in self.prerelease.iter().zip(&other.prerelease) {
                            let order = match (a.parse::<u64>(), b.parse::<u64>()) {
                                (Ok(a), Ok(b)) => a.cmp(&b),
                                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                                (Err(_), Err(_)) => a.cmp(b),
                            };
                            if order.is_ne() {
                                return order;
                            }
                        }
                        self.prerelease.len().cmp(&other.prerelease.len())
                    }
                })
        }
    }

    impl PartialOrd for Version<'_> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    /// A version with `x`, `X`, `*` or missing parts as `None`. Build metadata is dropped.
    struct Partial<'a> {
        major: Option<u64>,
        minor: Option<u64>,
        patch: Option<u64>,
        prerelease: Vec<&'a str>,
    }

    impl<'a> Partial<'a> {
        fn parse(text: &'a str) -> Option<Self> {
            let text = text.trim_start_matches(['v', '=']).trim();
            let text = text.split_once('+').map_or(text, |(version, _)| version);
            let (core, prerelease) = text.split_once('-').map_or((text, ""), |(c, p)| (c, p));
            let mut parts = core.split('.');
            let mut part = || -> Option<Option<u64>> {
                match parts.next() {
                    None | Some("x" | "X" | "*") => Some(None),
                    Some(number) => number.parse().ok().map(Some),
                }
            };
            let (major, minor, patch) = (part()?, part()?, part()?);
            if parts.next().is_some() {
                return None;
            }
            let prerelease =
                if prerelease.is_empty() { Vec::new() } else { prerelease.split('.').collect() };
            Some(Self { major, minor, patch, prerelease })
        }

        /// The lowest version the partial matches: missing parts are `0`.
        fn floor(self) -> Version<'a> {
            Version {
                major: self.major.unwrap_or(0),
                minor: self.minor.unwrap_or(0),
                patch: self.patch.unwrap_or(0),
                prerelease: self.prerelease,
            }
        }

        /// The lowest version above every version the partial matches.
        fn above(self) -> Version<'a> {
            let mut version = Version {
                major: self.major.unwrap_or(0),
                minor: self.minor.unwrap_or(0),
                patch: self.patch.unwrap_or(0),
                prerelease: Vec::new(),
            };
            if self.major.is_none() {
                return version;
            }
            if self.minor.is_none() {
                version.major += 1;
                version.minor = 0;
                version.patch = 0;
            } else if self.patch.is_none() {
                version.minor += 1;
                version.patch = 0;
            } else if self.prerelease.is_empty() {
                version.patch += 1;
            } else {
                version.prerelease = self.prerelease;
                version.prerelease.push("0");
            }
            version
        }
    }

    /// The lowest version that satisfies `range`, or `None` when `range` is not a valid range.
    /// Assumes every comparator set is satisfiable.
    pub fn min_version(range: &str) -> Option<Version<'_>> {
        range
            .split("||")
            .map(|set| {
                let set = set.trim();
                if let Some((from, _)) = set.split_once(" - ") {
                    return Partial::parse(from).map(Partial::floor);
                }
                let mut tokens = Vec::new();
                let mut pending_operator = "";
                for token in set.split_whitespace() {
                    if matches!(token, ">" | ">=" | "<" | "<=" | "=" | "^" | "~" | "~>") {
                        pending_operator = token;
                    } else if pending_operator.is_empty() {
                        tokens.push((None, token));
                    } else {
                        tokens.push((Some(pending_operator), token));
                        pending_operator = "";
                    }
                }
                let mut lower: Option<Version<'_>> = None;
                for (operator, token) in tokens {
                    let (operator, version) = match operator {
                        Some(operator) => (operator, token),
                        None => {
                            let split = token
                                .find(|c: char| !matches!(c, '<' | '>' | '=' | '^' | '~'))
                                .unwrap_or(token.len());
                            token.split_at(split)
                        }
                    };
                    let partial = Partial::parse(version)?;
                    let bound = match operator {
                        "<" | "<=" => continue,
                        ">" => partial.above(),
                        _ => partial.floor(),
                    };
                    if lower.as_ref().map_or(true, |lower| bound > *lower) {
                        lower = Some(bound);
                    }
                }
                Some(lower.unwrap_or(Version {
                    major: 0,
                    minor: 0,
                    patch: 0,
                    prerelease: Vec::new(),
                }))
            })
            .try_fold(None, |min: Option<Version<'_>>, set| {
                let set = set?;
                Some(Some(match min {
                    Some(min) if min <= set => min,
                    _ => set,
                }))
            })
            .flatten()
    }
}
