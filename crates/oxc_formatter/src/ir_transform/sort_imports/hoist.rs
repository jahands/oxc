// JHPATCH: Move top-level imports to the top of the file (sortImports.hoist) to match @ianvs/prettier-plugin-sort-imports
use std::{borrow::Cow, ops::Range};

use rustc_hash::FxHashMap;

use oxc_allocator::Allocator;
use oxc_ast::{
    Comment,
    ast::{Program, Statement},
};
use oxc_span::GetSpan;
use oxc_syntax::line_terminator::is_line_terminator;

use super::{
    SortImportsOptions,
    merge::{ImportRewrites, Rewrite, is_empty_named_import},
};

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
/// With `rewrites`, a merged import is written as its merged text, below its own comments
/// and the comments of the imports merged into it, and an import merged into another one is left out.
/// An import that is split, or merged into another one, owns no comments,
/// and the comments it would own (and those of the imports merged into a split import)
/// stay in the rest of the source or go to the next import.
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
    rewrites: Option<&ImportRewrites>,
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

    let keeps_comments = |idx: usize| rewrites.is_none_or(|rewrites| rewrites.keeps_comments(idx));
    let mut imports: Vec<HoistedImport> = vec![];
    // The comments owned by each import that is merged into an import that keeps its comments.
    let mut moved_comments: FxHashMap<usize, &[Comment]> = FxHashMap::default();
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
            if matches!(prev, Statement::ImportDeclaration(_)) && keeps_comments(idx - 1) {
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
        if idx == first_import_idx && keeps_comments(idx) {
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
        let trailing = if keeps_comments(idx) { &trailing[..trailing_count] } else { &[] };

        match rewrites.and_then(|rewrites| rewrites.get(idx)) {
            Some(Rewrite::Absorbed(into)) => {
                removed.push(decl.span.start..decl.span.end);
                if keeps_comments(*into) {
                    removed
                        .extend(leading.iter().map(|comment| comment.span.start..comment.span.end));
                    moved_comments.insert(idx, leading);
                }
                continue;
            }
            Some(Rewrite::Replace { split: Some(_), .. }) => {
                removed.push(decl.span.start..decl.span.end);
                imports.push(HoistedImport {
                    idx,
                    range: decl.span.start..decl.span.end,
                    decl_range: decl.span.start..decl.span.end,
                    leading: &[],
                });
                continue;
            }
            _ => {}
        }

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
        imports.push(HoistedImport {
            idx,
            range,
            decl_range: decl.span.start..decl.span.end,
            leading,
        });
    }
    removed.extend(top_comments.iter().map(|comment| comment.span.start..comment.span.end));
    removed.sort_unstable_by_key(|range| range.start);

    let text = |range: Range<u32>| &source[range.start as usize..range.end as usize];
    let moved_for = |import: &HoistedImport| -> Vec<&Comment> {
        match rewrites.and_then(|rewrites| rewrites.get(import.idx)) {
            Some(Rewrite::Replace { absorbed, split: None, .. }) => absorbed
                .iter()
                .filter_map(|idx| moved_comments.get(idx))
                .flat_map(|comments| comments.iter())
                .collect(),
            _ => vec![],
        }
    };
    let import_text = |import: &HoistedImport| -> ImportText<'_> {
        let Some(Rewrite::Replace { text: new_text, split, .. }) =
            rewrites.and_then(|rewrites| rewrites.get(import.idx))
        else {
            let range = import.range.clone();
            return ImportText {
                text: Cow::Borrowed(text(range.clone())),
                decl_offset: (import.decl_range.start - range.start) as usize,
                moved_lines: 0,
                joins_comment_when_first: false,
            };
        };
        if split.is_some() {
            return ImportText {
                text: Cow::Borrowed(new_text),
                decl_offset: 0,
                moved_lines: 0,
                joins_comment_when_first: false,
            };
        }
        let moved = moved_for(import);
        let mut moved_lines = moved.len();
        let mut joins_comment_when_first = false;
        let mut out = String::new();
        if moved.is_empty() {
            out.push_str(text(import.range.start..import.decl_range.start));
        } else {
            let mut last = import.leading.last();
            if let (Some(first), Some(last)) = (import.leading.first(), last) {
                out.push_str(text(first.span.start..last.span.end));
            }
            for &comment in &moved {
                if let Some(prev) = last {
                    let lines = if prev.span.end <= comment.span.start {
                        line_breaks(text(prev.span.end..comment.span.start)).clamp(1, 2)
                    } else {
                        1
                    };
                    out.push_str(&"\n".repeat(lines));
                }
                out.push_str(text(comment.span.start..comment.span.end));
                last = Some(comment);
            }
            // The import stays on the line of a single-line block comment that comes first.
            let first = import.leading.first().unwrap_or(moved[0]);
            let same_line = !first.is_line()
                && !first.is_multiline_block()
                && last.is_some_and(|last| !last.is_line());
            out.push(if same_line { ' ' } else { '\n' });
            moved_lines -= usize::from(same_line);
            joins_comment_when_first = !same_line && last.is_some_and(|last| !last.is_line());
        }
        let decl_offset = out.len();
        out.push_str(new_text);
        out.push_str(text(import.decl_range.end..import.range.end));
        ImportText { text: Cow::Owned(out), decl_offset, moved_lines, joins_comment_when_first }
    };

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
        (decl.specifiers.is_none() || is_empty_named_import(decl, comments))
            && comments_in(comments, prev_end, decl.span.start).is_empty()
    };
    let mut info = ImportRewriteInfo::default();
    let mut prev_import: Option<(usize, u32)> = None;
    for import in &imports {
        let (idx, range) = (import.idx, import.range.clone());
        let separator = match prev_import {
            Some((prev_idx, prev_end)) => {
                let between = text(prev_end..range.start);
                if prev_idx + 1 != idx {
                    // Keeps the imports on non-adjacent lines, as they were in the source.
                    "\n\n"
                } else if between.contains(is_line_terminator) && between.trim().is_empty() {
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
        let prev_idx_before = prev_import.map(|(prev_idx, _)| prev_idx);
        prev_import = Some((idx, range.end));
        let ImportText { text: import_text, decl_offset, moved_lines, joins_comment_when_first } =
            import_text(import);
        let decl_start = (out.len() + decl_offset) as u32;
        if joins_comment_when_first {
            info.joins_comment_when_first.push(decl_start);
        }
        if prev_idx_before.is_some_and(|prev_idx| prev_idx + 1 != idx) {
            info.detached_import_starts.push(decl_start);
        }
        if let Some(Rewrite::Replace { split: Some(type_offset), .. }) =
            rewrites.and_then(|rewrites| rewrites.get(idx))
        {
            info.split_import_starts.extend([decl_start, decl_start + *type_offset as u32]);
        }
        // The first import of the block keeps its own comments
        // when the first import of the source is merged into another import or split.
        let own_lines = if prev_idx_before.is_none() && idx != first_import_idx {
            import
                .leading
                .iter()
                .filter(|comment| has_line_break(source, comment.span.end, import.decl_range.start))
                .count()
        } else {
            0
        };
        if moved_lines + own_lines > 0 {
            info.moving_comments.push((decl_start, moved_lines + own_lines));
        }
        out.push_str(&import_text);
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
    Some(HoistedSource { source_text: allocator.alloc_str(&out), info })
}

/// The source rewritten by [`hoist_imports`].
pub struct HoistedSource<'a> {
    pub source_text: &'a str,
    pub info: ImportRewriteInfo,
}

/// What the import sorting needs to know about a source rewritten by [`hoist_imports`].
#[derive(Debug, Default)]
pub struct ImportRewriteInfo {
    /// Start offsets of the imports with comments on lines of their own directly above them
    /// that move with them when they are the first import, and the number of those comments.
    /// These are the comments from the imports merged into an import,
    /// and the comments of the first import of the block when the first import of the source
    /// is merged into another import or split.
    pub moving_comments: Vec<(u32, usize)>,
    /// Start offsets of the imports split from a value import with inline `type` specifiers.
    pub split_import_starts: Vec<u32>,
    /// Start offsets of the imports whose previous statement in the source is not the import before them.
    pub detached_import_starts: Vec<u32>,
    /// Start offsets of the imports with a block comment from a merged import directly above them,
    /// which go on the line of that comment when nothing is printed before them.
    pub joins_comment_when_first: Vec<u32>,
}

/// An import as written into the block at the top by [`hoist_imports`].
struct ImportText<'s> {
    text: Cow<'s, str>,
    /// Offset of the declaration in `text`.
    decl_offset: usize,
    /// Number of comments from merged imports on lines of their own above the declaration.
    moved_lines: usize,
    /// See [`ImportRewriteInfo::joins_comment_when_first`].
    joins_comment_when_first: bool,
}

/// A top-level import written into the block at the top.
struct HoistedImport<'c> {
    /// Body index of the import.
    idx: usize,
    /// The import with the comments it owns.
    range: Range<u32>,
    decl_range: Range<u32>,
    /// The comments it owns before it, below the comments that stay at the top.
    leading: &'c [Comment],
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
pub(super) fn comments_in(comments: &[Comment], start: u32, end: u32) -> &[Comment] {
    let from = comments.partition_point(|comment| comment.span.start < start);
    let to = comments.partition_point(|comment| comment.span.start < end);
    &comments[from..to]
}

pub(super) fn has_line_break(source: &str, start: u32, end: u32) -> bool {
    source[start as usize..end as usize].contains(is_line_terminator)
}

/// Number of line breaks in `text`, counting `\r\n` once.
pub(super) fn line_breaks(text: &str) -> usize {
    text.chars().filter(|&c| is_line_terminator(c)).count() - text.matches("\r\n").count()
}
