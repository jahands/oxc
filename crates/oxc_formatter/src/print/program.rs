use std::ops::Deref;

use oxc_allocator::ArenaVec;
use oxc_ast::ast::*;
use oxc_span::GetSpan;

use crate::{
    Format,
    ast_nodes::{AstNode, AstNodes},
    formatter::{prelude::*, trivia::FormatTrailingComments},
    ir_transform::{
        ChunkPosition, ChunkSource, HoistedImportFacts, ImportLines, sort_imports_chunk,
    },
    print::semicolon::{OptionalSemicolon, suppressed_statement_content_end},
    utils::{
        is_dropped_statement, statement_span,
        string::{FormatLiteralStringToken, StringLiteralParentKind},
    },
    write,
};

use super::FormatWrite;

impl<'a> FormatWrite<'a> for AstNode<'a, Program<'a>> {
    fn write(&self, f: &mut JsFormatter<'_, 'a>) {
        // BOM: JS is the exception to the entries-own-the-strip rule.
        // `format_program` is AST-in (the formatter never owns pre-parse text)
        // and oxc_parser lexes U+FEFF as whitespace itself.
        // Detect at print time, re-emit once at byte 0.
        let has_bom = oxc_formatter_core::spec::split_bom(f.source_text().as_str()).0;

        write!(
            f,
            [has_bom.then_some(text("\u{feff}")), FormatProgramBody(self), hard_line_break()]
        );
    }
}

/// A program without its envelope (BOM, trailing newline): the embedded root of [`crate::format_to_ir`],
/// whose host owns the layout around it.
pub struct FormatProgramBody<'a, 'b>(pub &'b AstNode<'a, Program<'a>>);

impl<'a> Format<'a, JsFormatContext<'a>> for FormatProgramBody<'a, '_> {
    fn fmt(&self, f: &mut JsFormatter<'_, 'a>) {
        let program = self.0;
        let format_trailing_comments = format_with(|f| {
            write!(
                f,
                FormatTrailingComments::Comments(f.context().comments().unprinted_comments())
            );
        });

        write!(
            f,
            [
                program.hashbang(),
                program.directives(),
                FormatStatementsWithImports(program.body()),
                format_trailing_comments
            ]
        );
    }
}

pub(super) struct FormatStatementsWithImports<'a, 'b>(
    pub &'b AstNode<'a, ArenaVec<'a, Statement<'a>>>,
);

impl<'a> Deref for FormatStatementsWithImports<'a, '_> {
    type Target = AstNode<'a, ArenaVec<'a, Statement<'a>>>;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> Format<'a, JsFormatContext<'a>> for FormatStatementsWithImports<'a, '_> {
    fn fmt(&self, f: &mut JsFormatter<'_, 'a>) {
        // JHPATCH: Chunk position, and sort only program-level imports with hoist (sortImports.hoist)
        // The comments before the first import start the file when hoisting moved code from above them.
        let program_position = match self.parent() {
            AstNodes::Program(program)
                if (program.hashbang.is_some() || !program.directives.is_empty())
                    && !f
                        .context()
                        .hoist_facts()
                        .is_some_and(|facts| facts.top_comments_followed_code) =>
            {
                ChunkPosition::AfterPrologue
            }
            AstNodes::Program(_) => ChunkPosition::FileStart,
            _ => ChunkPosition::Inner,
        };
        // With `hoist`, only the imports of the program body are sorted.
        let import_sort_enabled = f
            .options()
            .sort_imports
            .as_ref()
            .is_some_and(|options| !options.hoist || program_position != ChunkPosition::Inner);

        let mut join = f.join_nodes_with_hardline();

        let mut stmts_iter = self.iter().filter(|stmt| !is_dropped_statement(stmt.as_ref()));
        // JHPATCH: Only the first chunk can start the program (sortImports.hoist)
        let mut is_first = true;
        while let Some(mut stmt) = stmts_iter.next() {
            // JHPATCH: Keep suppressed imports in the run (sortImports.partitionBySideEffect)
            // Suppressed imports are emitted verbatim. They act as partition boundaries,
            // or with `partition_by_side_effect`, as imports that keep their position.
            if import_sort_enabled
                && matches!(stmt.as_ref(), Statement::ImportDeclaration(_))
                && (keeps_suppressed_imports(join.fmt()) || !is_import_suppressed(stmt, join.fmt()))
            {
                // JHPATCH: Only the first chunk can start the program (sortImports.hoist)
                let position = if is_first { program_position } else { ChunkPosition::Inner };
                let next_stmt =
                    format_import_decls_with_sort(stmt, &mut stmts_iter, &mut join, position);
                match next_stmt {
                    Some(next_stmt) => stmt = next_stmt,
                    None => break,
                }
            }

            // JHPATCH: Only the first chunk can start the program (sortImports.hoist)
            is_first = false;
            join.entry(statement_span(stmt.as_ref()), stmt);
        }
    }
}

/// Collect a run of consecutive `ImportDeclaration`s from `stmts_iter`, format them using `join`,
/// then sort them in place.
///
/// Returns the next statement after the run of `ImportDeclaration`s, or `None` if there are no more statements.
///
/// The caller must already have verified that `sort_imports` option is enabled.
///
/// # Panics
/// Panics if `sort_imports` option is not enabled.
//
// `#[cold]` because most statements aren't `ImportDeclaration`s.
// Also, when there *are* lots of `ImportDeclaration`s, they tend to all be grouped together.
// This function consumes the whole run, so is unlikely to be called more than once, even in files with lots of imports.
#[cold]
fn format_import_decls_with_sort<'a, 'iter>(
    stmt: &AstNode<'a, Statement<'a>>,
    stmts_iter: &mut impl Iterator<Item = &'iter AstNode<'a, Statement<'a>>>,
    join: &mut JoinNodesBuilder<'_, '_, 'a, Line>,
    // JHPATCH: Chunk position for the hoist comment rules (sortImports.hoist)
    position: ChunkPosition,
) -> Option<&'iter AstNode<'a, Statement<'a>>> {
    // Output inter-statement separator separately, so `chunk_start` points
    // to start of IR for the `ImportDeclaration` itself
    join.separator_no_entry(stmt.span());
    let chunk_start = join.fmt().elements().len();

    // Output first `ImportDeclaration`
    join.entry_no_separator(stmt);
    // JHPATCH: Start offsets of the imports in the run, to look up rewritten imports (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
    let mut import_starts = vec![stmt.span().start];

    // JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
    let mut import_lines = ImportLineCounter::new(join.fmt());
    import_lines.push(stmt.span(), None);

    // The first import was already written above, so start the count at 1.
    // JHPATCH: Keep suppressed imports in the run (sortImports.partitionBySideEffect)
    // A suppressed `ImportDeclaration` ends the run unless `partition_by_side_effect` keeps it in place.
    let keeps_suppressed = keeps_suppressed_imports(join.fmt());
    let mut count = 1;
    let mut next_stmt = None;
    // JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
    let mut prev_span = stmt.span();
    for stmt in stmts_iter {
        if let Statement::ImportDeclaration(decl) = stmt.as_ref() {
            // JHPATCH: Keep suppressed imports in the run (sortImports.partitionBySideEffect)
            if !keeps_suppressed && is_import_suppressed(stmt, join.fmt()) {
                next_stmt = Some(stmt);
                break;
            }
            join.entry(decl.span, stmt);
            // JHPATCH: Start offsets of the imports in the run, to look up rewritten imports (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
            import_starts.push(decl.span.start);
            count += 1;
            // JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
            import_lines.push(decl.span, Some(prev_span));
            prev_span = decl.span;
        } else {
            next_stmt = Some(stmt);
            break;
        }
    }

    // A single-import run is already in order, so skip the transform.
    // JHPATCH: The hoist comment rules also apply to a single import at the start of the program (sortImports.hoist)
    if count >= 2 || (import_lines.enabled && position != ChunkPosition::Inner) {
        // JHPATCH: Where the chunk sits in the source, for the hoist comment rules (sortImports.hoist)
        let facts = join.fmt().context().hoist_facts().filter(|_| position != ChunkPosition::Inner);
        let source = ChunkSource {
            position,
            imports: import_lines.lines,
            has_top_of_file_comments: facts.map(|facts| facts.has_top_of_file_comments),
            top_comment_gap: facts.map_or(1, |facts| facts.top_comment_gap),
            blank_before_first_import_comments: facts
                .is_some_and(|facts| facts.blank_before_first_import_comments),
            first_import_comment_height: facts.map_or(0, |facts| facts.first_import_comment_height),
        };
        // JHPATCH: Import offsets to look up rewritten imports (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
        sort_imports_chunk(join.fmt_mut(), chunk_start, &source, &import_starts);
    }

    next_stmt
}

// JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
/// Collects [`ImportLines`] for the imports of a run, with `sortImports.hoist`.
struct ImportLineCounter<'a> {
    source: &'a str,
    comments: &'a [Comment],
    /// Byte offset and line number of the last position counted.
    cursor: (usize, u32),
    lines: Vec<ImportLines>,
    enabled: bool,
    /// See [`crate::ir_transform::HoistFacts::imports`].
    hoisted_imports: Vec<HoistedImportFacts>,
}

impl<'a> ImportLineCounter<'a> {
    fn new(f: &JsFormatter<'_, 'a>) -> Self {
        Self {
            hoisted_imports: f
                .context()
                .hoist_facts()
                .map(|facts| facts.imports.clone())
                .unwrap_or_default(),
            source: f.source_text().as_str(),
            comments: f.context().comments().all_comments_before(u32::MAX),
            cursor: (0, 0),
            lines: vec![],
            enabled: f.options().sort_imports.as_ref().is_some_and(|options| options.hoist),
        }
    }

    /// Line of `pos`, which must not be before the last position counted.
    fn line_of(&mut self, pos: u32) -> u32 {
        let (offset, line) = self.cursor;
        let pos = (pos as usize).max(offset);
        #[expect(clippy::cast_possible_truncation)]
        let line = line + self.source[offset..pos].matches('\n').count() as u32;
        self.cursor = (pos, line);
        line
    }

    /// Add the import at `span`, which follows the import at `prev`, or starts the run.
    ///
    /// The import owns the comments before it that are not on the line where `prev` starts.
    /// The first import of a run owns the comments that end on its line.
    /// Its trailing comments start after it on the line where it starts.
    fn push(&mut self, span: Span, prev: Option<Span>) {
        if !self.enabled {
            return;
        }
        let source = self.source;
        let before = &self.comments[..self.comments.partition_point(|c| c.span.start < span.start)];
        let first_owned = match prev {
            Some(prev) => before[before.partition_point(|c| c.span.start < prev.end)..]
                .iter()
                .find(|comment| {
                    source[prev.start as usize..comment.span.start as usize].contains('\n')
                }),
            None => before
                .iter()
                .rev()
                .take_while(|comment| {
                    !source[comment.span.end as usize..span.start as usize].contains('\n')
                })
                .last(),
        };
        let trailing_comment_end = self.comments
            [self.comments.partition_point(|c| c.span.start < span.end)..]
            .iter()
            .take_while(|comment| {
                !source[span.start as usize..comment.span.start as usize].contains('\n')
            })
            .last()
            .map(|comment| comment.span.end);
        #[expect(clippy::cast_possible_truncation)]
        let trailing_comment_height = trailing_comment_end.map_or(0, |end| {
            source[span.start as usize..end as usize].matches('\n').count() as u32
        });
        let comment_line = first_owned.map(|comment| self.line_of(comment.span.start));
        let line = self.line_of(span.start);
        let comment_height = comment_line.map_or(0, |comment_line| line - comment_line);
        // Imports moved by `hoist` count by their place in the source before it.
        let hoisted = self
            .hoisted_imports
            .binary_search_by_key(&span.start, |import| import.start)
            .ok()
            .map(|idx| self.hoisted_imports[idx]);
        self.lines.push(ImportLines {
            line: hoisted.map_or(line, |import| import.line),
            comment_height,
            follows_import: hoisted.is_none_or(|import| import.follows_import),
            has_trailing_comment: trailing_comment_end.is_some(),
            trailing_comment_height,
            keeps_position: hoisted.is_some_and(|import| import.keeps_position),
        });
    }
}

// JHPATCH: Keep suppressed imports in the run (sortImports.partitionBySideEffect)
/// With `partition_by_side_effect`, suppressed imports stay in the sortable run,
/// where they keep their position like side effect imports.
fn keeps_suppressed_imports(f: &JsFormatter<'_, '_>) -> bool {
    f.options().sort_imports.as_ref().is_some_and(|options| options.partition_by_side_effect)
}

/// An `ImportDeclaration` is suppressed if it has a leading or trailing suppression comment,
/// which causes it to be emitted verbatim and act as a partition boundary, excluding it from the sortable run.
fn is_import_suppressed(stmt: &AstNode<'_, Statement<'_>>, f: &JsFormatter<'_, '_>) -> bool {
    f.comments()
        .is_node_suppressed(stmt.span(), || suppressed_statement_content_end(stmt.as_ref(), f))
}

impl<'a> Format<'a, JsFormatContext<'a>> for AstNode<'a, ArenaVec<'a, Directive<'a>>> {
    fn fmt(&self, f: &mut JsFormatter<'_, 'a>) {
        let Some(last_directive) = self.last() else {
            // No directives, no extra new line
            return;
        };

        // if next_sibling's first leading_trivia has more than one new_line,
        // we should add an extra empty line at the end of the last directive, for example:
        //```js
        // "use strict"; <- first leading new_line
        //  			 <- second leading new_line
        // function foo() {
        //
        // }
        //```
        // so we should keep an extra empty line after the last directive.

        // If the last directive has a trailing comment, `lines_after` stops at the first
        // non-whitespace character (`/`) and returns 0 before counting any newlines.
        // Only the LAST directive is checked here
        // (between-directive blanks go through `get_lines_before`, which is not subject to this hazard);
        // the per-comment-kind pins live in `tests/fixtures/js/directives/issue-21152*.js`, one file each.
        let check_pos = f
            .context()
            .comments()
            .end_of_line_comments_after(last_directive.span.end)
            .last()
            .map_or(last_directive.span.end, |c| c.span.end);
        let need_extra_empty_line = f.source_text().lines_after(check_pos) > 1;

        f.join_nodes_with_hardline().entries(self);

        write!(f, if need_extra_empty_line { empty_line() } else { hard_line_break() });
    }
}

impl<'a> FormatWrite<'a> for AstNode<'a, Directive<'a>> {
    fn write(&self, f: &mut JsFormatter<'_, 'a>) {
        write!(
            f,
            [
                FormatLiteralStringToken::new(
                    f.source_text().text_for(&self.expression),
                    /* jsx */
                    false,
                    StringLiteralParentKind::Directive,
                ),
                OptionalSemicolon
            ]
        );
    }
}

impl<'a> FormatWrite<'a> for AstNode<'a, Hashbang<'a>> {
    fn write(&self, f: &mut JsFormatter<'_, 'a>) {
        write!(f, ["#!", text(self.value().as_str().trim_end())]);

        if f.source_text().lines_after(self.span.end) > 1 {
            write!(f, [empty_line()]);
        } else {
            write!(f, [hard_line_break()]);
        }
    }
}
