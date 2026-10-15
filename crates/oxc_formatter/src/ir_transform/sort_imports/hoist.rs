use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::{
    Comment,
    ast::{Program, Statement},
};
use oxc_span::GetSpan;
use oxc_syntax::line_terminator::is_line_terminator;

use super::SortImportsOptions;

/// Rewrite the source text of `program` so that every top-level import sits in one block
/// at the top of the file, the way `@ianvs/prettier-plugin-sort-imports` lays out a file
/// before Prettier formats it.
///
/// The rewritten text holds, in order:
/// - the hashbang, then the comments that directly follow it
/// - the directives with the comments before them, a blank line, then the comments that directly follow them
/// - the comments before the first import that end on an earlier line than it ("top of file" comments)
/// - every top-level import with the comments it owns, in source order,
///   with a blank line between imports that were not adjacent statements
/// - a blank line, then the rest of the source with all of the above removed
///
/// An import owns the comments between the previous statement and itself,
/// except those on the line where the previous import starts, which belong to that import.
///
/// With `partition_by_side_effect` and a group boundary that inserts newlines,
/// a blank line separates the hashbang, or the comments after it or after the directives,
/// from a first side effect import that has no comments before it.
///
/// Returns `None` when the source already has this layout.
#[expect(clippy::cast_possible_truncation)] // Offsets fit in `u32`, source length does
pub fn hoist_imports<'a>(
    program: &Program<'a>,
    options: &SortImportsOptions,
    allocator: &'a Allocator,
) -> Option<&'a str> {
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

    // Ranges of each import with the comments it owns, and the body index of the import.
    let mut imports: Vec<(usize, Range<u32>)> = vec![];
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
        imports.push((idx, range));
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
        last_end = block.end;
        out.push_str(text(block));
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
        decl.specifiers.is_none() && comments_in(comments, prev_end, decl.span.start).is_empty()
    };
    let mut prev_import: Option<(usize, u32)> = None;
    for (idx, range) in imports {
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
        prev_import = Some((idx, range.end));
        out.push_str(text(range));
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

    if out == source.trim_end() {
        return None;
    }
    out.push('\n');
    Some(allocator.alloc_str(&out))
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
