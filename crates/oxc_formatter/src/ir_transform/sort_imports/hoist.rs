// JHPATCH: Move top-level imports to the top of the file (sortImports.hoist) to match @ianvs/prettier-plugin-sort-imports
use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::{
    Comment,
    ast::{ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName, Program, Statement},
};
use oxc_span::{GetSpan, Span};
use oxc_syntax::line_terminator::is_line_terminator;

use super::SortImportsOptions;

/// Rewrite the source text of `program` so that every top-level import sits in one block
/// at the top of the file, the way `@ianvs/prettier-plugin-sort-imports` lays out a file
/// before Prettier formats it.
///
/// The rewritten text holds, in order:
/// - the hashbang, then the comments that directly follow it
/// - the directives with the comments before them, one per line, with a blank line before
///   the comments between two directives, then a blank line and the comments that directly follow them
/// - the comments before the first import that end on an earlier line than it ("top of file" comments)
/// - every top-level import with the comments it owns, in source order,
///   with a blank line between imports that were not adjacent statements;
///   a `prettier-ignore`d import, `import {} from`, and an import with comments after its `with` clause
///   are written as Babel prints them ([`babel_import_text`])
/// - a blank line, then the rest of the source with all of the above removed
///
/// An import owns the comments between the previous statement and itself,
/// except those on the line where the previous import starts, which belong to that import.
///
/// With `partition_by_side_effect` and a group boundary that inserts newlines,
/// a blank line separates the hashbang, or the comments after it or after the directives,
/// from a first side effect import that has no comments before it.
///
/// Returns `None` when the program has no imports.
/// Also returns the [`HoistFacts`] that the import sorting needs about the source.
#[expect(clippy::cast_possible_truncation)] // Offsets fit in `u32`, source length does
pub fn hoist_imports<'a>(
    program: &Program<'a>,
    options: &SortImportsOptions,
    allocator: &'a Allocator,
) -> Option<HoistedSource<'a>> {
    let source = program.source_text;
    let body = &program.body;
    let first_import_idx =
        body.iter().position(|stmt| matches!(stmt, Statement::ImportDeclaration(_)))?;
    let comments = program.comments.as_slice();

    let bom_len = if source.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let body_start = body[0].span().start;
    let hashbang_end = program.hashbang.as_ref().map(|hashbang| hashbang.span.end);
    let head_end = program.directives.first().map_or(body_start, |directive| directive.span.start);

    let interpreter_comments = match hashbang_end {
        Some(hashbang_end) => comments_in(comments, hashbang_end, head_end),
        None => &[],
    };
    let directive_block = match (program.directives.first(), program.directives.last()) {
        (Some(first), Some(last)) => {
            let start = if hashbang_end.is_some() {
                first.span.start
            } else {
                comments_in(comments, 0, first.span.start)
                    .first()
                    .map_or(first.span.start, |comment| comment.span.start)
            };
            Some(start..last.span.end)
        }
        _ => None,
    };
    let directive_comments = match program.directives.last() {
        Some(last) => comments_in(comments, last.span.end, body_start),
        None => &[],
    };
    let has_prologue = hashbang_end.is_some() || directive_block.is_some();

    let mut removed: Vec<Range<u32>> = vec![];
    if let Some(hashbang) = &program.hashbang {
        removed.push(hashbang.span.start..hashbang.span.end);
    }
    removed.extend(interpreter_comments.iter().map(|comment| comment.span.start..comment.span.end));
    if let Some(block) = &directive_block {
        removed.push(block.clone());
    }
    removed.extend(directive_comments.iter().map(|comment| comment.span.start..comment.span.end));

    let prologue_end = program
        .directives
        .last()
        .map(|directive| directive.span.end)
        .or(hashbang_end)
        .unwrap_or(bom_len as u32);
    let mut imports: Vec<HoistedImport> = vec![];
    let mut top_comments: &[Comment] = &[];
    for (idx, stmt) in body.iter().enumerate() {
        let Statement::ImportDeclaration(decl) = stmt else {
            continue;
        };
        let start = decl.span.start;

        let mut leading = if idx == 0 {
            if has_prologue { &[] } else { comments_in(comments, bom_len as u32, start) }
        } else {
            let prev = &body[idx - 1];
            let gap = comments_in(comments, prev.span().end, start);
            if matches!(prev, Statement::ImportDeclaration(_)) {
                let owned_by_prev = gap
                    .iter()
                    .take_while(|comment| {
                        !has_line_break(source, prev.span().start, comment.span.start)
                    })
                    .count();
                &gap[owned_by_prev..]
            } else {
                gap
            }
        };
        // The comments that end up directly before the import, where Prettier reads `prettier-ignore`,
        // as it does in the comments after the import on its line.
        let is_ignored =
            if idx == 0 { comments_in(comments, prologue_end, start) } else { leading }
                .iter()
                .any(|comment| is_prettier_ignore(source, comment));
        if idx == first_import_idx {
            let top_count = leading
                .iter()
                .take_while(|comment| has_line_break(source, comment.span.end, start))
                .count();
            top_comments = &leading[..top_count];
            leading = &leading[top_count..];
        }

        let next_start = body.get(idx + 1).map_or(source.len() as u32, |next| next.span().start);
        let trailing = comments_in(comments, decl.span.end, next_start);
        let trailing_count = trailing
            .iter()
            .take_while(|comment| !has_line_break(source, start, comment.span.start))
            .count();
        let trailing = &trailing[..trailing_count];

        // The text between the owned comments and the import stays in the rest of the source.
        removed.extend(
            leading
                .iter()
                .chain(trailing)
                .map(|comment| comment.span.start..comment.span.end)
                .chain(std::iter::once(decl.span.start..decl.span.end)),
        );
        let range = leading.first().map_or(start, |comment| comment.span.start)
            ..trailing.last().map_or(decl.span.end, |comment| comment.span.end);
        let has_comment_after_with_clause = decl.with_clause.as_ref().is_some_and(|with_clause| {
            !comments_in(comments, with_clause.span.end, decl.span.end).is_empty()
        });
        let babel_text = if is_ignored
            || trailing.iter().any(|comment| is_prettier_ignore(source, comment))
            || has_comment_after_with_clause
            || decl.specifiers.as_ref().is_some_and(|s| s.is_empty())
        {
            babel_import_text(decl, source, comments)
        } else {
            None
        };
        // `@ianvs/prettier-plugin-sort-imports` keeps an import of nothing in place,
        // and one with `prettier-ignore` among all comments after the previous statement.
        let keeps_position = (decl.import_kind.is_type()
            && decl.specifiers.as_ref().is_some_and(|s| s.is_empty()))
            || (!is_ignored
                && idx > 0
                && comments_in(comments, body[idx - 1].span().end, start)
                    .iter()
                    .any(|comment| is_prettier_ignore(source, comment)));
        imports.push(HoistedImport { idx, range, babel_text, keeps_position });
    }
    removed.extend(top_comments.iter().map(|comment| comment.span.start..comment.span.end));
    removed.sort_unstable_by_key(|range| range.start);

    let text = |range: Range<u32>| &source[range.start as usize..range.end as usize];

    let mut out = String::with_capacity(source.len() + 2);
    out.push_str(&source[..bom_len]);

    let mut last = Last::Nothing;
    let mut last_end = 0;

    if let Some(hashbang) = &program.hashbang {
        out.push_str(text(hashbang.span.start..hashbang.span.end));
        last = Last::Hashbang;
        last_end = hashbang.span.end;
    }
    for comment in interpreter_comments {
        if last == Last::Hashbang && line_breaks(text(last_end..comment.span.start)) < 2 {
            out.push('\n');
        } else {
            out.push_str(text(last_end..comment.span.start));
        }
        out.push_str(text(comment.span.start..comment.span.end));
        last = Last::InterpreterComment;
        last_end = comment.span.end;
    }
    if let Some(block) = directive_block {
        if last != Last::Nothing {
            out.push('\n');
        }
        let directives = &program.directives;
        out.push_str(text(block.start..directives[0].span.start));
        for (idx, directive) in directives.iter().enumerate() {
            out.push_str(text(directive.span.start..directive.span.end));
            let Some(next) = directives.get(idx + 1) else {
                break;
            };
            // Babel puts each directive on its own line,
            // and a blank line before the comments that follow a directive.
            match comments_in(comments, directive.span.end, next.span.start) {
                [] => out.push('\n'),
                [first, .., last] | [first @ last] => {
                    out.push_str("\n\n");
                    out.push_str(text(first.span.start..last.span.end));
                    let gap = text(last.span.end..next.span.start);
                    out.push_str(if gap.contains(is_line_terminator) { gap } else { "\n" });
                }
            }
        }
        last_end = block.end;
        last = Last::Directives;
    }
    for comment in directive_comments {
        out.push_str(if last == Last::Directives {
            "\n\n"
        } else {
            text(last_end..comment.span.start)
        });
        out.push_str(text(comment.span.start..comment.span.end));
        last = Last::DirectiveComment;
        last_end = comment.span.end;
    }
    for comment in top_comments {
        out.push_str(match last {
            Last::Nothing => "",
            Last::Directives => "\n\n",
            Last::Hashbang | Last::InterpreterComment | Last::DirectiveComment => "\n",
            Last::TopComment => text(last_end..comment.span.start),
        });
        out.push_str(text(comment.span.start..comment.span.end));
        last = Last::TopComment;
        last_end = comment.span.end;
    }

    let separates_side_effects = options.partition_by_side_effect
        && !options.sort_side_effects
        && (options.newlines_between || options.newline_boundary_overrides.contains(&Some(true)));
    // A side effect import with no comment between the previous statement (or prologue) and itself.
    let is_bare_side_effect = |idx: usize| {
        let Statement::ImportDeclaration(decl) = &body[idx] else {
            return false;
        };
        let prev_end = match idx.checked_sub(1) {
            Some(prev_idx) => body[prev_idx].span().end,
            None => program
                .directives
                .last()
                .map(|directive| directive.span.end)
                .or(hashbang_end)
                .unwrap_or(bom_len as u32),
        };
        decl.specifiers
            .as_ref()
            .is_none_or(|specifiers| specifiers.is_empty() && !decl.import_kind.is_type())
            && comments_in(comments, prev_end, decl.span.start).is_empty()
    };
    let mut import_facts = vec![];
    // Byte offset and line number in the source of the last import start counted.
    let mut line_cursor = (0, 0);
    let mut prev_import: Option<(usize, u32)> = None;
    for HoistedImport { idx, range, babel_text, keeps_position } in imports {
        let separator = match prev_import {
            Some((prev_idx, prev_end)) => {
                let between = text(prev_end..range.start);
                if prev_idx + 1 != idx {
                    // Keeps the imports on non-adjacent lines, as they were in the source.
                    "\n\n"
                } else if between.contains(is_line_terminator) {
                    between
                } else {
                    "\n"
                }
            }
            None => match last {
                Last::Nothing => "",
                Last::Directives => "\n\n",
                Last::Hashbang | Last::InterpreterComment | Last::DirectiveComment
                    if separates_side_effects && is_bare_side_effect(idx) =>
                {
                    "\n\n"
                }
                Last::Hashbang | Last::InterpreterComment | Last::DirectiveComment => "\n",
                Last::TopComment => text(last_end..range.start),
            },
        };
        out.push_str(separator);
        let decl_start = body[idx].span().start as usize;
        line_cursor = (decl_start, line_cursor.1 + line_breaks(&source[line_cursor.0..decl_start]));
        import_facts.push(HoistedImportFacts {
            start: (out.len() + decl_start - range.start as usize) as u32,
            line: line_cursor.1 as u32,
            follows_import: prev_import.is_none_or(|(prev_idx, _)| prev_idx + 1 == idx),
            keeps_position,
        });
        prev_import = Some((idx, range.end));
        match babel_text {
            Some(babel_text) => {
                let decl_span = body[idx].span();
                out.push_str(text(range.start..decl_span.start));
                out.push_str(&babel_text);
                out.push_str(text(decl_span.end..range.end));
            }
            None => out.push_str(text(range)),
        }
    }

    let mut rest = String::new();
    let mut kept_from = bom_len as u32;
    for range in removed {
        if kept_from < range.start {
            rest.push_str(text(kept_from..range.start));
        }
        kept_from = kept_from.max(range.end);
    }
    rest.push_str(&source[kept_from as usize..]);
    let rest = rest.trim();
    if !rest.is_empty() {
        out.push_str("\n\n");
        out.push_str(rest);
    }

    let source_text = (out != source.trim_end()).then(|| {
        out.push('\n');
        allocator.alloc_str(&out)
    });
    let first_import_start = body[first_import_idx].span().start;
    // After a prologue, the comments before the first import that end on its line belong to it.
    let (top_of_file_comments, first_import_comments) = if has_prologue && first_import_idx == 0 {
        let prologue_comments = comments_in(comments, prologue_end, first_import_start);
        let top_count = prologue_comments
            .iter()
            .rposition(|comment| has_line_break(source, comment.span.end, first_import_start))
            .map_or(0, |idx| idx + 1);
        prologue_comments.split_at(top_count)
    } else {
        (top_comments, &[][..])
    };
    let top_comment_gap = top_of_file_comments
        .last()
        .map_or(1, |comment| line_breaks(text(comment.span.end..first_import_start)) as u32);
    let blank_before_first_import_comments =
        match (top_of_file_comments.last(), first_import_comments.first()) {
            (Some(last_top), Some(first)) => {
                top_comment_gap >= 2 && line_breaks(text(last_top.span.end..first.span.start)) == 1
            }
            _ => false,
        };
    let first_import_comment_height = first_import_comments
        .first()
        .map_or(0, |comment| line_breaks(text(comment.span.start..first_import_start)) as u32);
    Some(HoistedSource {
        source_text,
        facts: HoistFacts {
            top_comments_followed_code: has_prologue
                && first_import_idx > 0
                && !top_comments.is_empty(),
            has_top_of_file_comments: !top_of_file_comments.is_empty(),
            top_comment_gap,
            blank_before_first_import_comments,
            first_import_comment_height,
            imports: import_facts,
        },
    })
}

/// The source text written by [`hoist_imports`].
pub struct HoistedSource<'a> {
    /// `None` when the source already has the layout.
    pub source_text: Option<&'a str>,
    pub facts: HoistFacts,
}

/// What the source looked like before [`hoist_imports`] rewrote it,
/// for the comment and blank line rules of the import sorting.
#[derive(Clone, Debug, Default)]
pub struct HoistFacts {
    /// Whether statements stood between the prologue (hashbang or directives)
    /// and the "top of file" comments of the first import.
    /// Those comments then count as the start of the file, not as comments of the prologue.
    pub top_comments_followed_code: bool,
    /// Whether comments before the first import end on an earlier line than it,
    /// after the prologue or the previous statement ("top of file" comments).
    pub has_top_of_file_comments: bool,
    /// Lines from the end of the last "top of file" comment to the first import. 1 without such comments.
    pub top_comment_gap: u32,
    /// Whether the first import directly follows a hashbang or directives, and its comments
    /// start on the line after the "top of file" comments and end on a later line.
    /// Babel then separates the two with a blank line when that import is sorted first.
    pub blank_before_first_import_comments: bool,
    /// Lines from the start of the comments of the first import that directly follows a hashbang
    /// or directives to the import. Those comments stay after the prologue in the rewritten source.
    pub first_import_comment_height: u32,
    /// The top-level imports, in order.
    pub imports: Vec<HoistedImportFacts>,
}

/// An import moved by [`hoist_imports`].
#[derive(Clone, Copy, Debug)]
pub struct HoistedImportFacts {
    /// Offset of the import declaration in the rewritten source.
    pub start: u32,
    /// Line of the import declaration in the source.
    pub line: u32,
    /// Whether the previous statement in the source was an import, or there was none.
    pub follows_import: bool,
    /// Whether the import keeps its position like a side effect import.
    pub keeps_position: bool,
}

/// A top-level import of the source.
struct HoistedImport {
    /// Index of the import in the program body.
    idx: usize,
    /// The import with the comments it owns.
    range: Range<u32>,
    /// The text that replaces the import declaration.
    babel_text: Option<String>,
    /// See [`HoistedImportFacts::keeps_position`].
    keeps_position: bool,
}

/// The last part written before the imports.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    Nothing,
    Hashbang,
    InterpreterComment,
    Directives,
    DirectiveComment,
    TopComment,
}

/// The import as `@babel/generator` prints it, which `@ianvs/prettier-plugin-sort-imports` writes
/// before Prettier formats the file: on one line, `import {} from` as a side effect import.
/// The text after the source or `with` clause (a semicolon, comments) is kept,
/// with the comments after a `with` clause moved before it.
/// `None` when comments sit inside the import, or it has an import phase or an empty `with` clause.
fn babel_import_text(
    decl: &ImportDeclaration,
    source: &str,
    comments: &[Comment],
) -> Option<String> {
    let clause_end = decl.with_clause.as_ref().map_or(decl.source.span.end, |w| w.span.end);
    if decl.phase.is_some()
        || decl.with_clause.as_ref().is_some_and(|with_clause| with_clause.with_entries.is_empty())
        || !comments_in(comments, decl.span.start, clause_end).is_empty()
    {
        return None;
    }
    let text = |span: Span| &source[span.start as usize..span.end as usize];

    let mut out = String::from("import ");
    if decl.import_kind.is_type() {
        out.push_str("type ");
    }
    let specifiers = decl.specifiers.as_ref().map_or(&[][..], |specifiers| specifiers.as_slice());
    let named_start = specifiers
        .iter()
        .position(|specifier| matches!(specifier, ImportDeclarationSpecifier::ImportSpecifier(_)))
        .unwrap_or(specifiers.len());
    for (idx, specifier) in specifiers[..named_start].iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        match specifier {
            ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                out.push_str(&default.local.name);
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                out.push_str("* as ");
                out.push_str(&namespace.local.name);
            }
            ImportDeclarationSpecifier::ImportSpecifier(_) => {}
        }
    }
    let named = &specifiers[named_start..];
    if !named.is_empty() {
        if named_start > 0 {
            out.push_str(", ");
        }
        out.push_str("{ ");
        for (idx, specifier) in named.iter().enumerate() {
            let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier else {
                return None;
            };
            if idx > 0 {
                out.push_str(", ");
            }
            if specifier.import_kind.is_type() {
                out.push_str("type ");
            }
            match &specifier.imported {
                ModuleExportName::StringLiteral(literal) => out.push_str(text(literal.span)),
                imported => out.push_str(&imported.name()),
            }
            if !matches!(&specifier.imported, ModuleExportName::IdentifierName(name) if name.name == specifier.local.name)
            {
                out.push_str(" as ");
                out.push_str(&specifier.local.name);
            }
        }
        out.push_str(" }");
    } else if decl.import_kind.is_type() && specifiers.is_empty() {
        out.push_str("{}");
    }
    if !specifiers.is_empty() || decl.import_kind.is_type() {
        out.push_str(" from ");
    }
    out.push_str(text(decl.source.span));
    let tail_comments = comments_in(comments, clause_end, decl.span.end);
    let mut tail_start = clause_end;
    if let Some(with_clause) = &decl.with_clause {
        // Babel prints the comments between the `with` clause and the semicolon after the source.
        if let (Some(first), Some(last)) = (tail_comments.first(), tail_comments.last()) {
            out.push(' ');
            out.push_str(text(Span::new(first.span.start, last.span.end)));
            tail_start = last.span.end;
        }
        out.push_str(" with { ");
        for (idx, entry) in with_clause.with_entries.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(text(entry.key.span()));
            out.push_str(": ");
            out.push_str(text(entry.value.span));
        }
        out.push_str(" }");
    }
    out.push_str(text(Span::new(tail_start, decl.span.end)));
    Some(out)
}

/// Whether `comment` is `prettier-ignore`, as `@ianvs/prettier-plugin-sort-imports` reads it.
fn is_prettier_ignore(source: &str, comment: &Comment) -> bool {
    let content = comment.content_span();
    source[content.start as usize..content.end as usize].trim() == "prettier-ignore"
}

/// The comments that start in `start..end`, which hold no statement.
fn comments_in(comments: &[Comment], start: u32, end: u32) -> &[Comment] {
    let from = comments.partition_point(|comment| comment.span.start < start);
    let to = comments.partition_point(|comment| comment.span.start < end);
    &comments[from..to]
}

fn has_line_break(source: &str, start: u32, end: u32) -> bool {
    source[start as usize..end as usize].contains(is_line_terminator)
}

/// Number of line breaks in `text`, counting `\r\n` once.
fn line_breaks(text: &str) -> usize {
    text.chars().filter(|&c| is_line_terminator(c)).count() - text.matches("\r\n").count()
}
