// JHPATCH: Merge duplicate imports and split inline type specifiers (sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers) to match @ianvs/prettier-plugin-sort-imports
use std::ops::Range;

use rustc_hash::FxHashMap;

use oxc_allocator::Allocator;
use oxc_ast::{
    Comment,
    ast::{ImportDeclaration, ImportDeclarationSpecifier, Program, Statement},
};
use oxc_formatter_core::spec::is_suppression_marker;
use oxc_span::{GetSpan, Span};
use oxc_syntax::line_terminator::is_line_terminator;

use super::{
    SortImportsOptions,
    collation::{compare_names, normalize_name},
    compute_metadata::is_style,
    group_matcher::GroupMatcher,
    hoist::{comments_in, has_line_break, line_breaks},
};

/// The rewrites of top-level imports for `merge_duplicates` and `split_type_specifiers`,
/// keyed by the index of the import in the program body.
pub struct ImportRewrites {
    rewrites: FxHashMap<usize, Rewrite>,
}

pub enum Rewrite {
    /// The import is printed as `text`.
    Replace {
        text: String,
        /// Body indices of the imports merged into this one, in merge order.
        absorbed: Vec<usize>,
        /// When the import is split, `text` holds the value import, a line break and the type import,
        /// which starts at this byte offset in `text`.
        split: Option<usize>,
    },
    /// The import is merged into the import at this body index.
    Absorbed(usize),
}

impl ImportRewrites {
    pub fn get(&self, idx: usize) -> Option<&Rewrite> {
        self.rewrites.get(&idx)
    }

    /// Whether the import at `idx` is printed as a declaration that keeps its comments:
    /// it is neither merged into another import nor split.
    pub fn keeps_comments(&self, idx: usize) -> bool {
        !matches!(
            self.rewrites.get(&idx),
            Some(Rewrite::Absorbed(_) | Rewrite::Replace { split: Some(_), .. })
        )
    }
}

/// Plan the merges and splits of the top-level imports of `program`,
/// following `@ianvs/prettier-plugin-sort-imports`.
///
/// Imports are merged within the runs of imports that are sorted together:
/// every top-level import with `hoist`, otherwise each run of consecutive imports.
/// Suppressed imports end a run, and so do side effect imports that keep their position with `partition_by_side_effect`,
/// blank lines with `partition_by_newline`, and comment lines with `partition_by_comment`.
///
/// Within a run, the first import of each source takes in the later ones,
/// value imports first, then type imports.
/// Value and type imports share one table of first imports only without `split_type_specifiers`;
/// a type import that is merged into a value import (or takes one in) has its named specifiers marked `type`.
/// An import with a namespace specifier, two imports with a default specifier,
/// and a type import with a default specifier are not merged, and the first import keeps its place in the table.
/// Imports with different import attributes are not merged.
///
/// With `split_type_specifiers`, a value import with more than one specifier,
/// some but not all of them inline `type` specifiers, is then split into a value import and a type import.
/// An import with import attributes is not split.
///
/// Imports with comments outside their `{ ... }` or inside a specifier are left as written.
///
/// Returns `None` when no import changes.
pub fn plan_import_rewrites(
    program: &Program<'_>,
    options: &SortImportsOptions,
) -> Option<ImportRewrites> {
    let source = program.source_text;
    let comments = program.comments.as_slice();
    let body = &program.body;
    let group_matcher = GroupMatcher::new(&options.groups, &options.custom_groups);
    let prologue_end = program
        .directives
        .last()
        .map(|directive| directive.span.end)
        .or(program.hashbang.as_ref().map(|hashbang| hashbang.span.end))
        .unwrap_or(0);

    let mut runs: Vec<Vec<Candidate>> = vec![vec![]];
    let mut prev_import: Option<(usize, &ImportDeclaration)> = None;
    for (idx, stmt) in body.iter().enumerate() {
        let Statement::ImportDeclaration(decl) = stmt else {
            if !options.hoist {
                end_run(&mut runs);
            }
            continue;
        };

        if let Some((prev_idx, prev)) = prev_import
            && ends_run_between(source, comments, options, prev_idx, prev, idx, decl)
        {
            end_run(&mut runs);
        }
        prev_import = Some((idx, decl));

        let prev_end = idx.checked_sub(1).map_or(prologue_end, |prev| body[prev].span().end);
        let is_suppressed = comments_in(comments, prev_end, decl.span.start)
            .iter()
            .chain(same_line_comments_after(source, comments, decl.span.end))
            .any(|comment| is_suppression_marker(comment.content_span().source_text(source)));
        let keeps_position = decl.specifiers.is_none()
            && !options.sort_side_effects
            && !group_matcher.should_regroup_side_effect()
            && (!decl.source.value.as_str().is_some_and(is_style)
                || !group_matcher.should_regroup_side_effect_style());
        if is_suppressed || (keeps_position && options.partition_by_side_effect) {
            end_run(&mut runs);
            continue;
        }
        if let Some(candidate) = Candidate::new(idx, decl, source, comments) {
            runs.last_mut().expect("`runs` is never empty").push(candidate);
        }
    }

    let mut rewrites = FxHashMap::default();
    for mut run in runs {
        if options.merge_duplicates {
            merge_run(&mut run, !options.split_type_specifiers);
        }
        for candidate in run {
            if candidate.is_absorbed {
                continue;
            }
            let should_split = options.split_type_specifiers && candidate.should_split();
            if candidate.absorbed.is_empty() && !should_split {
                continue;
            }
            let (text, split) = if should_split {
                let (text, type_offset) = candidate.render_split(source, options);
                (text, Some(type_offset))
            } else {
                (candidate.render(source, options), None)
            };
            for &absorbed in &candidate.absorbed {
                rewrites.insert(absorbed, Rewrite::Absorbed(candidate.idx));
            }
            rewrites.insert(
                candidate.idx,
                Rewrite::Replace { text, absorbed: candidate.absorbed, split },
            );
        }
    }

    (!rewrites.is_empty()).then_some(ImportRewrites { rewrites })
}

fn end_run(runs: &mut Vec<Vec<Candidate<'_>>>) {
    if runs.last().is_some_and(|run| !run.is_empty()) {
        runs.push(vec![]);
    }
}

/// Whether a partition boundary of sorting separates two imports that are next to each other in a run.
fn ends_run_between(
    source: &str,
    comments: &[Comment],
    options: &SortImportsOptions,
    prev_idx: usize,
    prev: &ImportDeclaration,
    idx: usize,
    decl: &ImportDeclaration,
) -> bool {
    let gap = comments_in(comments, prev.span.end, decl.span.start);
    (options.partition_by_newline
        && (prev_idx + 1 != idx || has_blank_line(source, gap, prev.span.end, decl.span.start)))
        || (options.partition_by_comment
            && gap.iter().any(|comment| has_line_break(source, prev.span.end, comment.span.start)))
}

/// Merge the imports of `run` the way `@ianvs/prettier-plugin-sort-imports` does.
///
/// `combine` lets type imports and value imports of the same source merge.
fn merge_run(run: &mut [Candidate<'_>], combine: bool) {
    let mut first_by_source: FxHashMap<&str, usize> = FxHashMap::default();
    for is_type in [false, true] {
        if !combine {
            first_by_source.clear();
        }
        for pos in 0..run.len() {
            if run[pos].is_type_flavor != is_type {
                continue;
            }
            let Some(&existing) = first_by_source.get(run[pos].source_value) else {
                first_by_source.insert(run[pos].source_value, pos);
                continue;
            };
            // The import that comes first in the source is kept,
            // unless it starts the source (`@ianvs/prettier-plugin-sort-imports` reads offset 0 as no offset).
            if run[pos].start != 0 && run[existing].start > run[pos].start {
                if merge_into(run, pos, existing) {
                    first_by_source.insert(run[pos].source_value, pos);
                }
            } else {
                merge_into(run, existing, pos);
            }
        }
    }
}

/// Merge `run[forget]` into `run[keep]`, unless the merged import would be invalid.
fn merge_into(run: &mut [Candidate<'_>], keep: usize, forget: usize) -> bool {
    let (kept, forgotten) = (&run[keep], &run[forget]);
    let is_safe = kept.is_rebuildable
        && forgotten.is_rebuildable
        && !kept.has_namespace
        && !forgotten.has_namespace
        && !(kept.default.is_some() && forgotten.default.is_some())
        && !(kept.is_type && kept.default.is_some())
        && !(forgotten.is_type && forgotten.default.is_some())
        && kept.attributes == forgotten.attributes;
    if !is_safe {
        return false;
    }

    let forgotten_idx = forgotten.idx;
    let forgotten_is_type = forgotten.is_type;
    let default = forgotten.default;
    let named = std::mem::take(&mut run[forget].named);
    let absorbed = std::mem::take(&mut run[forget].absorbed);
    run[forget].is_absorbed = true;

    let kept = &mut run[keep];
    kept.is_type &= forgotten_is_type;
    kept.default = kept.default.or(default);
    kept.named.extend(named);
    kept.absorbed.push(forgotten_idx);
    kept.absorbed.extend(absorbed);
    true
}

/// An import that can take part in merging and splitting.
struct Candidate<'a> {
    /// Index in the program body.
    idx: usize,
    /// Start offset after the byte order mark.
    start: u32,
    source_value: &'a str,
    /// The source as written, with its quotes.
    source_text: &'a str,
    /// The import attributes as written with a leading space (` with { type: 'json' }`), or empty.
    attributes: &'a str,
    /// Whether the import was a type import before merging.
    is_type_flavor: bool,
    is_type: bool,
    default: Option<DefaultSpecifier<'a>>,
    has_namespace: bool,
    named: Vec<NamedSpecifier<'a>>,
    /// Whether every comment in the import is inside `{ ... }` and outside the specifiers,
    /// so that the import can be printed from its parts.
    is_rebuildable: bool,
    /// Body indices of the imports merged into this one, in merge order.
    absorbed: Vec<usize>,
    /// Whether this import is merged into another one.
    is_absorbed: bool,
}

#[derive(Clone, Copy)]
struct DefaultSpecifier<'a> {
    text: &'a str,
    span: Span,
}

/// A comment inside `{ ... }`.
#[derive(Clone, Copy)]
struct SpecifierComment<'a> {
    text: &'a str,
    span: Span,
}

impl SpecifierComment<'_> {
    fn is_line(self) -> bool {
        self.text.starts_with("//")
    }
}

/// A named specifier with the comments that Babel attaches to it.
struct NamedSpecifier<'a> {
    /// The specifier as written, without an inline `type` keyword.
    text: &'a str,
    local: &'a str,
    span: Span,
    /// Whether the specifier imports a type: an inline `type` specifier or a specifier of a type import.
    is_type: bool,
    /// Comments after `{` or a comma that come before the specifier.
    leading: Vec<SpecifierComment<'a>>,
    /// Comments directly after the specifier, and comments after the last comma of the list.
    trailing: Vec<SpecifierComment<'a>>,
}

impl<'a> Candidate<'a> {
    fn new(
        idx: usize,
        decl: &ImportDeclaration<'a>,
        source: &'a str,
        comments: &[Comment],
    ) -> Option<Self> {
        let specifiers = decl.specifiers.as_ref().filter(|specifiers| !specifiers.is_empty())?;
        if decl.phase.is_some() {
            return None;
        }

        let text = |range: Range<u32>| &source[range.start as usize..range.end as usize];
        let is_type = decl.import_kind.is_type();
        let mut default = None;
        let mut has_namespace = false;
        let mut named = vec![];
        for specifier in specifiers {
            match specifier {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                    default = Some(DefaultSpecifier {
                        text: text(specifier.local.span.start..specifier.local.span.end),
                        span: specifier.span,
                    });
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => has_namespace = true,
                ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                    named.push(NamedSpecifier {
                        text: text(specifier.imported.span().start..specifier.span.end),
                        local: specifier.local.name.as_str(),
                        span: specifier.span,
                        is_type: is_type || specifier.import_kind.is_type(),
                        leading: vec![],
                        trailing: vec![],
                    });
                }
            }
        }

        let is_rebuildable = attach_comments(
            &mut named,
            source,
            comments,
            comments_in(comments, decl.span.start, decl.span.end),
        );

        let attributes = if decl.with_clause.is_some() {
            text(decl.source.span.end..decl.span.end).trim_end().trim_end_matches(';').trim_end()
        } else {
            ""
        };

        Some(Self {
            idx,
            start: decl.span.start - if source.starts_with('\u{feff}') { 3 } else { 0 },
            source_value: decl.source.value.as_str()?,
            source_text: text(decl.source.span.start..decl.source.span.end),
            attributes,
            is_type_flavor: is_type,
            is_type,
            default,
            has_namespace,
            named,
            is_rebuildable,
            absorbed: vec![],
            is_absorbed: false,
        })
    }

    /// Whether a value import has more than one specifier, some but not all of them `type` specifiers.
    fn should_split(&self) -> bool {
        let type_count = self.named.iter().filter(|specifier| specifier.is_type).count();
        let specifier_count = self.named.len() + usize::from(self.default.is_some());
        !self.is_type
            && self.is_rebuildable
            && self.attributes.is_empty()
            && specifier_count > 1
            && type_count > 0
            && type_count < specifier_count
    }

    fn render(&self, source: &str, options: &SortImportsOptions) -> String {
        let declarations = [Declaration {
            is_type: self.is_type,
            default: self.default,
            named: self.named.iter().collect(),
        }];
        let [text] =
            render_declarations(declarations, self.source_text, self.attributes, source, options);
        text
    }

    /// The value import and the type import, and the offset of the type import.
    fn render_split(&self, source: &str, options: &SortImportsOptions) -> (String, usize) {
        let (types, values): (Vec<_>, Vec<_>) =
            self.named.iter().partition(|specifier| specifier.is_type);
        let declarations = [
            Declaration { is_type: false, default: self.default, named: values },
            Declaration { is_type: true, default: None, named: types },
        ];
        let [mut text, type_text] =
            render_declarations(declarations, self.source_text, self.attributes, source, options);
        text.push('\n');
        let type_offset = text.len();
        text.push_str(&type_text);
        (text, type_offset)
    }
}

/// Attach the comments in an import (`decl_comments`) to its named specifiers the way Babel does.
///
/// A comment directly after a specifier trails it.
/// A comment after `{` or a comma leads the next specifier, or trails the last one when `}` follows.
/// Comments with only whitespace between them are attached together.
///
/// Returns whether every comment is inside `{ ... }` and outside the specifiers.
fn attach_comments<'a>(
    named: &mut [NamedSpecifier<'a>],
    source: &'a str,
    comments: &[Comment],
    decl_comments: &[Comment],
) -> bool {
    if decl_comments.is_empty() {
        return true;
    }
    let (Some(first), Some(last)) = (named.first(), named.last()) else {
        return false;
    };
    let open = skip_back_over_comments(source, comments, first.span.start);
    let close = skip_over_comments(source, comments, last.span.end);
    let is_inside = |comment: &Comment| {
        comment.span.start >= open
            && comment.span.end <= close
            && !named.iter().any(|specifier| {
                specifier.span.start < comment.span.start && comment.span.end < specifier.span.end
            })
    };
    if !decl_comments.iter().all(is_inside) {
        return false;
    }

    let mut from = 0;
    while from < decl_comments.len() {
        let mut to = from + 1;
        while to < decl_comments.len()
            && source
                [decl_comments[to - 1].span.end as usize..decl_comments[to].span.start as usize]
                .trim()
                .is_empty()
        {
            to += 1;
        }
        let block = &decl_comments[from..to];
        from = to;

        #[expect(clippy::cast_possible_truncation)]
        let before = source[..block[0].span.start as usize].trim_end().len() as u32;
        let block_end = block[block.len() - 1].span.end as usize;
        #[expect(clippy::cast_possible_truncation)]
        let after = (source.len() - source[block_end..].trim_start().len()) as u32;
        let block = block.iter().map(|comment| SpecifierComment {
            text: &source[comment.span.start as usize..comment.span.end as usize],
            span: comment.span,
        });
        if let Some(prev) = named.iter_mut().find(|specifier| specifier.span.end == before) {
            prev.trailing.extend(block);
        } else if let Some(next) = named.iter_mut().find(|specifier| specifier.span.start == after)
        {
            next.leading.extend(block);
        } else {
            named.last_mut().expect("`named` is not empty").trailing.extend(block);
        }
    }
    true
}

/// An import declaration to print from specifiers.
struct Declaration<'s, 'a> {
    is_type: bool,
    default: Option<DefaultSpecifier<'a>>,
    named: Vec<&'s NamedSpecifier<'a>>,
}

/// A comment with the key that orders the comments placed at one spot:
/// the kind of attachment (trailing on the line of the specifier, leading, trailing on a later line),
/// the index among the comments of that kind of its specifier, and the output position of its specifier.
type PlacedComment<'a> = ((u8, usize, usize), SpecifierComment<'a>);

/// Comments placed around one specifier of the printed declarations.
#[derive(Default)]
struct PlacedComments<'a> {
    /// Comments printed before the specifier.
    leading: Vec<PlacedComment<'a>>,
    /// Comments printed after the specifier on its line.
    trailing: Vec<PlacedComment<'a>>,
}

/// Print `declarations`, which come in this order in the output.
///
/// With `sort_specifiers`, the named specifiers are ordered like `sort_named_specifiers` in the import printer.
///
/// Comments are placed the way `@ianvs/prettier-plugin-sort-imports` reattaches them:
/// - A trailing comment on the line of its specifier stays after it.
///   Other trailing comments go below the last specifier of the declaration.
/// - A leading comment on the line where a different specifier starts (the first one in the output)
///   goes after that specifier. Other leading comments stay before their specifier.
fn render_declarations<const N: usize>(
    mut declarations: [Declaration<'_, '_>; N],
    source_text: &str,
    attributes: &str,
    source: &str,
    options: &SortImportsOptions,
) -> [String; N] {
    if options.sort_specifiers {
        for declaration in &mut declarations {
            let is_type = declaration.is_type;
            declaration.named.sort_by(|a, b| {
                (!is_type && a.is_type).cmp(&(!is_type && b.is_type)).then_with(|| {
                    let ordering = compare_names(
                        &normalize_name(a.local, options),
                        &normalize_name(b.local, options),
                        options,
                    );
                    if options.order.is_desc() { ordering.reverse() } else { ordering }
                })
            });
        }
    }

    // The specifiers in output order: declaration index, named index (`None` for the default), and start.
    let outputs: Vec<(usize, Option<usize>, u32)> = declarations
        .iter()
        .enumerate()
        .flat_map(|(decl_idx, declaration)| {
            let default = declaration.default.map(|default| (decl_idx, None, default.span.start));
            let named = declaration.named.iter().enumerate().map(move |(named_idx, specifier)| {
                (decl_idx, Some(named_idx), specifier.span.start)
            });
            default.into_iter().chain(named)
        })
        .collect();
    let same_line = |a: u32, b: u32| !has_line_break(source, a.min(b), a.max(b));

    let mut placed: Vec<PlacedComments> =
        outputs.iter().map(|_| PlacedComments::default()).collect();
    let mut after: [Vec<PlacedComment>; N] = std::array::from_fn(|_| vec![]);
    for (pos, &(decl_idx, named_idx, start)) in outputs.iter().enumerate() {
        let Some(named_idx) = named_idx else { continue };
        let specifier = declarations[decl_idx].named[named_idx];
        for (counter, &comment) in specifier.trailing.iter().enumerate() {
            if same_line(start, comment.span.start) {
                placed[pos].trailing.push(((0, counter, pos), comment));
            } else {
                after[decl_idx].push(((2, counter, pos), comment));
            }
        }
        for (counter, &comment) in specifier.leading.iter().enumerate() {
            let owner =
                outputs.iter().position(|&(_, _, start)| same_line(start, comment.span.start));
            match owner {
                Some(owner) if owner != pos => {
                    placed[owner].trailing.push(((1, counter, pos), comment));
                }
                _ => placed[pos].leading.push(((1, counter, pos), comment)),
            }
        }
    }
    for comments in &mut placed {
        comments.leading.sort_by_key(|(key, _)| *key);
        comments.trailing.sort_by_key(|(key, _)| *key);
    }
    for comments in &mut after {
        comments.sort_by_key(|(key, _)| *key);
    }

    let mut texts = std::array::from_fn(|_| String::new());
    let mut pos = 0;
    for (decl_idx, declaration) in declarations.iter().enumerate() {
        let out = &mut texts[decl_idx];
        out.push_str("import ");
        if declaration.is_type {
            out.push_str("type ");
        }
        if let Some(default) = declaration.default {
            out.push_str(default.text);
            let trailing = &placed[pos].trailing;
            push_trailing_comments(out, trailing);
            if trailing.last().is_some_and(|(_, comment)| comment.is_line()) {
                out.push('\n');
            }
            pos += 1;
            if !declaration.named.is_empty() {
                out.push_str(", ");
            }
        }

        let named_placed = &placed[pos..pos + declaration.named.len()];
        let has_comments = !after[decl_idx].is_empty()
            || named_placed
                .iter()
                .any(|comments| !comments.leading.is_empty() || !comments.trailing.is_empty());
        if has_comments {
            out.push('{');
            let mut after_line_comment = false;
            for (specifier, comments) in declaration.named.iter().zip(named_placed) {
                // Babel prints a single-line block comment that comes first on the line of the `{` or comma before it.
                let first_on_prev_line = !after_line_comment
                    && comments.leading.first().is_some_and(|(_, comment)| {
                        !comment.is_line() && !comment.text.contains(is_line_terminator)
                    });
                out.push(if first_on_prev_line { ' ' } else { '\n' });
                push_leading_comments(out, &comments.leading, specifier.span.start, source);
                if !declaration.is_type && specifier.is_type {
                    out.push_str("type ");
                }
                out.push_str(specifier.text);
                push_trailing_comments(out, &comments.trailing);
                after_line_comment =
                    comments.trailing.last().is_some_and(|(_, comment)| comment.is_line());
                if after_line_comment {
                    out.push('\n');
                }
                out.push(',');
            }
            out.push('\n');
            for (_, comment) in &after[decl_idx] {
                out.push_str(comment.text);
                out.push('\n');
            }
            out.push('}');
        } else if !declaration.named.is_empty() {
            out.push_str("{ ");
            for (i, specifier) in declaration.named.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if !declaration.is_type && specifier.is_type {
                    out.push_str("type ");
                }
                out.push_str(specifier.text);
            }
            out.push_str(" }");
        }
        pos += declaration.named.len();

        out.push_str(" from ");
        out.push_str(source_text);
        if !attributes.is_empty() {
            out.push(' ');
            out.push_str(attributes.trim_start());
        }
    }
    texts
}

/// Print `comments` before a specifier that starts at `specifier_start`, with the line breaks Babel prints.
fn push_leading_comments(
    out: &mut String,
    comments: &[PlacedComment<'_>],
    specifier_start: u32,
    source: &str,
) {
    let (Some((_, first)), Some((_, last))) = (comments.first(), comments.last()) else {
        return;
    };
    let mut prev_end = None;
    for (_, comment) in comments {
        if let Some(prev_end) = prev_end {
            out.push(if has_line_break(source, prev_end, comment.span.start) { '\n' } else { ' ' });
        }
        out.push_str(comment.text);
        prev_end = Some(comment.span.end);
    }
    let breaks_after = first.is_line()
        || first.text.contains(is_line_terminator)
        || last.is_line()
        || has_line_break(source, last.span.end, specifier_start);
    out.push(if breaks_after { '\n' } else { ' ' });
}

/// Print `comments` after a specifier on its line.
fn push_trailing_comments(out: &mut String, comments: &[PlacedComment<'_>]) {
    let mut after_line_comment = false;
    for (_, comment) in comments {
        out.push(if after_line_comment { '\n' } else { ' ' });
        out.push_str(comment.text);
        after_line_comment = comment.is_line();
    }
}

/// Apply `rewrites` to the source of `program` in place, for sorting without `hoist`.
///
/// A merged import takes the comments directly above the imports merged into it.
/// The comments after a split import on its line stay with the value import.
/// Returns the rewritten source.
pub fn apply_import_rewrites<'a>(
    program: &Program<'a>,
    rewrites: &ImportRewrites,
    allocator: &'a Allocator,
) -> &'a str {
    let source = program.source_text;
    let comments = program.comments.as_slice();
    let body = &program.body;
    let text = |range: Range<u32>| &source[range.start as usize..range.end as usize];

    // The comments that move from each merged import to the import it is merged into.
    let mut moved: FxHashMap<usize, &[Comment]> = FxHashMap::default();
    let mut removed: Vec<Range<u32>> = vec![];
    for (idx, stmt) in body.iter().enumerate() {
        let Statement::ImportDeclaration(decl) = stmt else {
            continue;
        };
        let Some(Rewrite::Absorbed(_)) = rewrites.get(idx) else {
            continue;
        };
        if let Some(prev) = idx.checked_sub(1).map(|prev| &body[prev]) {
            let attached = attached_comments(source, comments, prev, decl.span.start);
            removed.extend(attached.iter().map(|comment| line_range(source, comment.span)));
            moved.insert(idx, attached);
        }
        removed.push(line_range(source, decl.span));
    }

    let mut edits: Vec<(Range<u32>, String)> =
        removed.into_iter().map(|range| (range, String::new())).collect();
    for (idx, stmt) in body.iter().enumerate() {
        let (
            Statement::ImportDeclaration(decl),
            Some(Rewrite::Replace { text: new_text, absorbed, split }),
        ) = (stmt, rewrites.get(idx))
        else {
            continue;
        };
        let mut replacement = String::new();
        for comment in absorbed.iter().filter_map(|idx| moved.get(idx)).flat_map(|c| c.iter()) {
            replacement.push_str(text(comment.span.start..comment.span.end));
            replacement.push('\n');
        }
        let mut end = decl.span.end;
        match (split, same_line_comments_after(source, comments, decl.span.end).last()) {
            (Some(type_offset), Some(last)) => {
                end = last.span.end;
                replacement.push_str(&new_text[..type_offset - 1]);
                replacement.push_str(text(decl.span.end..end));
                replacement.push('\n');
                replacement.push_str(&new_text[*type_offset..]);
            }
            _ => replacement.push_str(new_text),
        }
        edits.push((decl.span.start..end, replacement));
    }
    edits.sort_unstable_by_key(|(range, _)| range.start);

    let mut out = String::with_capacity(source.len());
    let mut kept_from = 0;
    for (range, replacement) in edits {
        if kept_from < range.start {
            out.push_str(text(kept_from..range.start));
        }
        out.push_str(&replacement);
        kept_from = kept_from.max(range.end);
    }
    out.push_str(&source[kept_from as usize..]);
    allocator.alloc_str(&out)
}

/// The comments on the lines directly above an import, with no blank line between them and the import,
/// that are not on the line where the previous statement ends.
fn attached_comments<'c>(
    source: &str,
    comments: &'c [Comment],
    prev: &Statement<'_>,
    start: u32,
) -> &'c [Comment] {
    let gap = comments_in(comments, prev.span().end, start);
    let own_line = gap
        .iter()
        .position(|comment| has_line_break(source, prev.span().end, comment.span.start))
        .unwrap_or(gap.len());
    let gap = &gap[own_line..];
    let mut from = gap.len();
    let mut next_start = start;
    while from > 0
        && line_breaks(&source[gap[from - 1].span.end as usize..next_start as usize]) <= 1
    {
        from -= 1;
        next_start = gap[from].span.start;
    }
    &gap[from..]
}

/// `span` widened to its whole line, with the line break, when nothing else is on its line.
/// A comment after `span` on its line stays.
#[expect(clippy::cast_possible_truncation)] // Offsets fit in `u32`, source length does
fn line_range(source: &str, span: Span) -> Range<u32> {
    let bytes = source.as_bytes();
    let mut start = span.start as usize;
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let at_line_start = start == 0 || matches!(bytes[start - 1], b'\n' | b'\r');
    if !at_line_start {
        return span.start..span.end;
    }
    let mut end = span.end as usize;
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if source[end..].starts_with("\r\n") {
        end += 2;
    } else if end < bytes.len() && matches!(bytes[end], b'\n' | b'\r') {
        end += 1;
    } else if end < bytes.len() {
        return start as u32..span.end;
    }
    start as u32..end as u32
}

/// The comments after `pos` on its line, with only whitespace before them.
fn same_line_comments_after<'c>(source: &str, comments: &'c [Comment], pos: u32) -> &'c [Comment] {
    let from = comments.partition_point(|comment| comment.span.start < pos);
    let count = comments[from..]
        .iter()
        .scan(pos, |end, comment| {
            let between = &source[*end as usize..comment.span.start as usize];
            if !between.trim().is_empty() || between.contains(is_line_terminator) {
                return None;
            }
            *end = comment.span.end;
            Some(())
        })
        .count();
    &comments[from..from + count]
}

/// The position before the whitespace and comments that end at `pos`.
fn skip_back_over_comments(source: &str, comments: &[Comment], mut pos: u32) -> u32 {
    loop {
        let trimmed = source[..pos as usize].trim_end();
        #[expect(clippy::cast_possible_truncation)]
        let end = trimmed.len() as u32;
        match comments.iter().rfind(|comment| comment.span.end == end) {
            Some(comment) => pos = comment.span.start,
            None => return end,
        }
    }
}

/// The position after the whitespace, commas and comments that start at `pos`.
fn skip_over_comments(source: &str, comments: &[Comment], mut pos: u32) -> u32 {
    loop {
        let rest = &source[pos as usize..];
        let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        #[expect(clippy::cast_possible_truncation)]
        let start = pos + (rest.len() - trimmed.len()) as u32;
        match comments.iter().find(|comment| comment.span.start == start) {
            Some(comment) => pos = comment.span.end,
            None => return start,
        }
    }
}

/// Whether a line in `start..end` holds only whitespace; `comments` are the comments in that range.
fn has_blank_line(source: &str, comments: &[Comment], start: u32, end: u32) -> bool {
    let mut from = start;
    for comment in comments {
        if line_breaks(&source[from as usize..comment.span.start as usize]) > 1 {
            return true;
        }
        from = comment.span.end;
    }
    line_breaks(&source[from as usize..end as usize]) > 1
}
