// JHPATCH: Intl collation module (sortImports.collation)
pub mod collation;
mod compute_metadata;
mod group_config;
mod group_matcher;
// JHPATCH: Import hoisting module (sortImports.hoist)
mod hoist;
pub mod options;
mod partitioned_chunk;
mod sortable_imports;
mod source_line;

use oxc_allocator::{Allocator, ArenaVec};
use oxc_formatter_core::format_element::{
    FormatElement, LineMode,
    tag::{LabelId, Tag},
};

// JHPATCH: Import hoisting entry point (sortImports.hoist)
pub use hoist::hoist_imports;

use crate::{
    JsLabels, SortImportsOptions,
    formatter::JsFormatter,
    ir_transform::sort_imports::{
        group_matcher::GroupMatcher, partitioned_chunk::PartitionedChunk,
        sortable_imports::SortableImport, source_line::SourceLine,
    },
};

/// Sort a chunk of `ImportDeclaration`s and replace the original `FormatElement`s with the sorted ones
/// in the formatter buffer.
///
/// `chunk_start` is the buffer position of the start of the chunk.
/// i.e. the buffer position recorded just before the first `entry` call for first `ImportDeclaration` in the chunk.
///
/// The chunk buffer must contain nothing but the `FormatElement`s generated from `ImportDeclaration`s,
/// and their preceding/trailing comments, and line breaks.
///
/// The chunk slice does not include any trailing line break, and `transform` produces a slice with
/// the same shape. The inter-statement separator is written by the next `entry` call
/// (or the closing newline emitted by `Program::write`).
///
/// The caller must already have verified that `sort_imports` option is enabled.
///
/// # Panics
/// Panics if `sort_imports` option is not enabled.
// JHPATCH: Pass the chunk position for the hoist comment rules (sortImports.hoist)
pub fn sort_imports_chunk(
    formatter: &mut JsFormatter<'_, '_>,
    chunk_start: usize,
    position: ChunkPosition,
) {
    let elements = &formatter.elements()[chunk_start..];
    let options = formatter.options().sort_imports.as_ref().unwrap();

    let sorted_elements = transform(elements, options, formatter.allocator(), position);
    formatter.replace_end(chunk_start, &sorted_elements);
}

// JHPATCH: Chunk position for the hoist comment rules (sortImports.hoist)
/// Where a chunk of `ImportDeclaration`s starts, for the comment rules of `hoist`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkPosition {
    /// The chunk does not start the program body.
    Inner,
    /// The chunk starts the program body, after a hashbang or directives.
    AfterPrologue,
    /// The chunk starts the program body, and nothing precedes it but comments.
    FileStart,
}

/// An IR transform that sorts import statements according to specified options.
/// Heavily inspired by ESLint's `@perfectionist/sort-imports` rule.
/// <https://perfectionist.dev/rules/sort-imports>
///
/// Transform the given slice of `FormatElement`s by sorting import statements according
/// to the specified options. The slice is one chunk of consecutive `ImportDeclaration`s
/// as written to the formatter buffer (delimited by the streaming driver in `FormatProgramBody`).
///
// NOTE: The input `FormatElement`s are already well-formatted.
// It means that:
// - There is no redundant spaces, no consecutive line breaks, etc...
// - The slice does not end with a line break (the inter-statement separator is written
//   by the next `entry` call, not as part of the chunk).
fn transform<'a>(
    elements: &[FormatElement<'a>],
    options: &SortImportsOptions,
    allocator: &'a Allocator,
    // JHPATCH: Chunk position for the hoist comment rules (sortImports.hoist)
    position: ChunkPosition,
) -> ArenaVec<'a, FormatElement<'a>> {
    // Parse string based groups into our internal representation for performance
    let group_matcher = GroupMatcher::new(&options.groups, &options.custom_groups);
    let prev_elements: &[FormatElement<'a>] = elements;

    // Roughly speaking, sort-imports is a process of swapping lines.
    // Therefore, as a preprocessing, group IR elements into line first.
    // e.g.
    // ```text
    // [Text, Space, Text, Line, StartTag, Text, Text, EndTag, Line, ...]
    // ```
    // ↓↓
    // ```text
    // [ [Text, Space, Text], [StartTag, Text, Text, EndTag], [...] ]
    // ```
    //
    // This is also meaningful to explicitly handle comment line, empty line,
    // and other line with or without import statement.
    //
    // NOTE: `FormatElement::Line(_)` may not exactly correspond to an actual line break in the output.
    // e.g. `LineMode::SoftOrSpace` may be rendered as a space.
    //
    // And this implementation is based on the following assumptions:
    // - Only `Line(Hard|Empty)` is used for joining `Program.body` in the output
    // - `Line(Hard|Empty)` does not appear inside an `ImportDeclaration` formatting
    //   - If this is the case, we should check `Tag::StartLabelled(JsLabels::ImportDeclaration)`
    let mut lines = vec![];
    let mut current_line_start = 0;
    // Track labelled blocks whose internal `Line(Hard)` must NOT trigger a line flush,
    // so the whole block is captured as a single `SourceLine`.
    // Classification of that captured line is then handled by `SourceLine::from_element_range`.
    //
    // `ImportDeclaration` can wrap an internal `hard_line_break()` for a multiline import:
    // ```text
    // import {
    //   a,
    // } from "foo";
    // ```
    let mut inside_comment = false;
    let mut inside_multiline_import = false;
    // Trailing comments emitted via `line_suffix(...)` live inside `StartLineSuffix..End`,
    // and any `Line(_)` inside is for output positioning, not a real line boundary.
    // e.g. ```import "a"; <StartLineSuffix> Line(Empty) Text("// trailing") <EndLineSuffix>```
    let mut inside_line_suffix = false;
    // JHPATCH: Line-splitting state for comments with hoist (sortImports.hoist)
    // A block comment that starts the file is followed by `Line(SoftOrSpace)`,
    // which prints as a line break at the top level.
    // With `hoist`, it splits lines so that the comment can stay at the top.
    let mut after_top_level_comment = false;
    // A trailing comment on its own line after the last statement is a line suffix
    // that starts with a line break. With `hoist`, it is a line of its own,
    // so that it stays after the imports.
    let mut inside_own_line_suffix = false;
    let comment_label = LabelId::of(JsLabels::Comment);
    let import_label = LabelId::of(JsLabels::ImportDeclaration);
    // JHPATCH: Read suppressed imports as imports (sortImports.partitionBySideEffect)
    let suppressed_import_label = LabelId::of(JsLabels::SuppressedImportDeclaration);
    for (idx, el) in prev_elements.iter().enumerate() {
        if let FormatElement::Tag(tag) = el {
            match tag {
                Tag::StartLabelled(id) => {
                    // JHPATCH: Treat a suppressed import like a multi-line import (sortImports.partitionBySideEffect)
                    if *id == comment_label {
                        inside_comment = true;
                    } else if *id == import_label || *id == suppressed_import_label {
                        inside_multiline_import = true;
                    }
                }
                // JHPATCH: Split an own-line trailing comment into its own line (sortImports.hoist)
                Tag::StartLineSuffix => {
                    inside_line_suffix = true;
                    if options.hoist
                        && !inside_comment
                        && !inside_multiline_import
                        && matches!(
                            prev_elements.get(idx + 1),
                            Some(FormatElement::Line(LineMode::Hard | LineMode::Empty))
                        )
                    {
                        inside_own_line_suffix = true;
                        if current_line_start < idx {
                            lines.push(SourceLine::from_element_range(
                                prev_elements,
                                current_line_start..idx,
                                LineMode::Hard,
                            ));
                        }
                        current_line_start = idx;
                    }
                }
                Tag::EndLineSuffix => {
                    inside_line_suffix = false;
                    if std::mem::take(&mut inside_own_line_suffix) {
                        lines.push(SourceLine::from_element_range(
                            prev_elements,
                            current_line_start..idx + 1,
                            LineMode::Hard,
                        ));
                        current_line_start = idx + 1;
                    }
                }
                Tag::EndLabelled => {
                    // `EndLabelled` doesn't carry the label ID.
                    // Neither label is expected to nest, so this naive reset is sufficient.
                    if inside_comment {
                        inside_comment = false;
                        // JHPATCH: Track a top-level block comment (sortImports.hoist)
                        after_top_level_comment =
                            options.hoist && !inside_multiline_import && !inside_line_suffix;
                        continue;
                    } else if inside_multiline_import {
                        inside_multiline_import = false;
                    }
                }
                _ => {}
            }
        }

        // JHPATCH: End a line after a top-level block comment (sortImports.hoist)
        let ends_line = match el {
            FormatElement::Line(LineMode::Empty | LineMode::Hard) => true,
            FormatElement::Line(LineMode::SoftOrSpace) => after_top_level_comment,
            _ => false,
        };
        after_top_level_comment = false;
        if let FormatElement::Line(mode) = el
            && ends_line
        {
            // Don't flush mid-block, the line break is internal to the label.
            if inside_comment || inside_multiline_import || inside_line_suffix {
                continue;
            }

            // All flushed lines terminate with `Hard`.
            // The `Empty` semantic is captured separately
            // as a `SourceLine::Empty` push for boundary detection downstream.
            if current_line_start < idx {
                lines.push(SourceLine::from_element_range(
                    prev_elements,
                    current_line_start..idx,
                    LineMode::Hard,
                ));
            }
            current_line_start = idx + 1;

            if matches!(mode, LineMode::Empty) {
                lines.push(SourceLine::Empty);
            }
        }
    }
    // The chunk doesn't end with a `Line(Hard)`,
    // so the in-loop flush above won't catch the last line.
    if current_line_start < prev_elements.len() {
        debug_assert!(
            !inside_comment && !inside_multiline_import && !inside_line_suffix,
            "Unbalanced labelled tags at end of chunk"
        );
        lines.push(SourceLine::from_element_range(
            prev_elements,
            current_line_start..prev_elements.len(),
            LineMode::Hard,
        ));
    }

    // Next, partition `SourceLine`s into `PartitionedChunk`s.
    //
    // Chunking is done by detecting boundaries.
    // By default, no boundary exists within an import run, so the whole chunk is sorted as one unit.
    // Depending on options, empty lines (`partition_by_newline`) and comment-only lines
    // (`partition_by_comment`) can also be boundaries.
    //
    // Within each chunk, we will sort import lines.
    // e.g. with `partition_by_newline: true`:
    // ```text
    // import C from "c"; // chunk1
    // import B from "b"; // chunk1
    //
    // import Z from "z"; // chunk2
    // import A from "a"; // chunk2
    // ```
    // ↓↓
    // ```text
    // import B from "b"; // chunk1
    // import C from "c"; // chunk1
    //
    // import A from "a"; // chunk2
    // import Z from "z"; // chunk2
    // ```
    let mut chunks = vec![];
    let mut current_chunk = PartitionedChunk::default();
    for line in lines {
        match line {
            // `SourceLine::Import` never be a boundary.
            SourceLine::Import(..) => {
                current_chunk.add_imports_line(line);
            }
            // `SourceLine::Empty` and `SourceLine::CommentOnly` can be boundaries depending on options.
            // Otherwise, they will be the leading/trailing lines of `PartitionedChunk::Imports`.
            SourceLine::Empty if !options.partition_by_newline => {
                current_chunk.add_imports_line(line);
            }
            // TODO: Support more flexible comment handling?
            // e.g. Specific text by regex, only line comments, etc.
            SourceLine::CommentOnly(..) if !options.partition_by_comment => {
                current_chunk.add_imports_line(line);
            }
            // This `SourceLine` is a boundary!
            // Reached only when `partition_by_newline` is true (for `Empty`)
            // or `partition_by_comment` is true (for `CommentOnly`).
            _ => {
                // Flush current import chunk
                if !current_chunk.is_empty() {
                    chunks.push(std::mem::take(&mut current_chunk));
                }
                // Add boundary chunk
                chunks.push(PartitionedChunk::Boundary(line));
            }
        }
    }
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    // Finally, sort import lines within each chunk.
    // After sorting, flatten everything back to `FormatElement`s.
    let mut next_elements = ArenaVec::with_capacity_in(prev_elements.len(), &allocator);

    // JHPATCH: Keep leading comments at the top of the file (sortImports.hoist)
    // With `hoist`, comments before the first import of the file stay at the top.
    let mut keep_leading_comments_at_top = options.hoist && position != ChunkPosition::Inner;
    let mut chunks_iter = chunks.into_iter().peekable();
    while let Some(chunk) = chunks_iter.next() {
        match chunk {
            // Boundary chunks: Just output as-is
            PartitionedChunk::Boundary(line) => {
                line.write(prev_elements, &mut next_elements, true);
            }
            // Import chunks: Sort and output
            PartitionedChunk::Imports(_) => {
                // Convert `ImportChunk` into `SortableImport`s.
                //
                // `SortableImport` is a logical unit of 1 import statement with its leading lines.
                // `OrphanContent` tracks comments separated by empty lines with their slot positions.
                //
                // Comments attach based on empty line separation:
                // - Comments directly before an `import` (no empty line) → `SortableImport.leading_lines`
                // - Comments followed by an empty line → `orphan_contents` (stay at slot position)
                //
                // e.g.
                // ```text
                // // orphan (after_slot: None)
                //
                // // leading for A
                // import A from "a";
                // // orphan (after_slot: Some(0))
                //
                // // leading for B
                // import B from "b";
                // // chunk trailing
                // ```
                // JHPATCH: Pass keep_leading_comments_at_top (sortImports.hoist)
                let (sorted_imports, orphan_contents, trailing_lines, slot_had_leading_blank) =
                    chunk.into_sorted_import_units(
                        &group_matcher,
                        options,
                        std::mem::take(&mut keep_leading_comments_at_top),
                    );

                // Output leading orphan content (after_slot: None)
                // JHPATCH: Track comments kept at the top (sortImports.hoist)
                let mut leading_orphan_ends_with_comment = false;
                let mut has_top_comments = false;
                for orphan in &orphan_contents {
                    if orphan.after_slot.is_none() {
                        for line in &orphan.lines {
                            line.write(prev_elements, &mut next_elements, true);
                        }
                        // JHPATCH: Track comments kept at the top (sortImports.hoist)
                        leading_orphan_ends_with_comment =
                            matches!(orphan.lines.last(), Some(SourceLine::CommentOnly(..)));
                        has_top_comments = options.hoist
                            && orphan
                                .lines
                                .iter()
                                .any(|line| matches!(line, SourceLine::CommentOnly(..)));
                    }
                }
                // JHPATCH: Blank line between top comments and the first import's comments (sortImports.hoist)
                // With `hoist`, comments kept at the top of the file are separated by a blank line
                // from the comments of an import sorted to the front.
                if has_top_comments
                    && position == ChunkPosition::FileStart
                    && leading_orphan_ends_with_comment
                    && sorted_imports.first().is_some_and(|first| !first.leading_lines.is_empty())
                {
                    next_elements.push(FormatElement::Line(LineMode::Empty));
                }
                // Below comments kept at the top, the first import counts as starting
                // on the line of the first import in the source, after its own leading lines.
                let first_source_line_idx =
                    sorted_imports.iter().map(|import| import.line_idx).min().unwrap_or_default();
                let mut prev_line_idx = 0;

                // Output sorted import units with orphan content at their slot positions
                // JHPATCH: Track the previous import for side effect and hoist blank lines
                let mut prev_import: Option<&SortableImport> = None;
                let mut seen_non_ignored = false;
                for (slot_idx, sorted_import) in sorted_imports.iter().enumerate() {
                    // JHPATCH: Blank lines around side effect imports (sortImports.partitionBySideEffect)
                    let insert_newline = prev_import.is_some_and(|prev| {
                        if options.partition_by_side_effect
                            && (prev.is_ignored || sorted_import.is_ignored)
                        {
                            return should_insert_newline_around_side_effect(
                                options,
                                prev,
                                sorted_import,
                            );
                        }
                        // Insert newline when:
                        // 1. Group changes
                        // 2. At least one non-ignored import has already been output
                        //    (leading ignored imports don't trigger group boundaries)
                        // 3. The boundary override (or global `newlines_between`) says to insert
                        //    For decreasing transitions, fall back to whether the original input
                        //    had a blank line at this slot position (matches perfectionist).
                        prev.group_idx != sorted_import.group_idx
                            && seen_non_ignored
                            && should_insert_newline_between(
                                options.newlines_between,
                                &options.newline_boundary_overrides,
                                prev.group_idx,
                                sorted_import.group_idx,
                                slot_had_leading_blank[slot_idx],
                            )
                    });
                    // JHPATCH: Blank line after an import with a trailing comment (sortImports.hoist)
                    // With `hoist`, an import with a trailing comment is followed by a blank line
                    // when the next import did not start on the line right after it.
                    let insert_newline = insert_newline
                        || (options.hoist
                            && prev_import.is_some_and(|prev| {
                                sorted_import.line_idx > prev_line_idx + 1
                                    && prev.has_trailing_comment
                            }));
                    prev_line_idx = if slot_idx == 0 && has_top_comments {
                        first_source_line_idx + sorted_import.leading_lines.len()
                    } else {
                        sorted_import.line_idx
                    };
                    if insert_newline {
                        next_elements.push(FormatElement::Line(LineMode::Empty));
                    }
                    // JHPATCH: Track the previous import for side effect and hoist blank lines
                    prev_import = Some(sorted_import);
                    if !sorted_import.is_ignored {
                        seen_non_ignored = true;
                    }

                    // Output leading lines and import line
                    for line in &sorted_import.leading_lines {
                        line.write(
                            prev_elements,
                            &mut next_elements,
                            // JHPATCH: Keep blank lines in leading comments (sortImports.hoist)
                            options.partition_by_newline || options.hoist,
                        );
                    }
                    sorted_import.import_line.write(prev_elements, &mut next_elements, false);

                    // Output orphan content that belongs after this slot
                    for orphan in &orphan_contents {
                        if orphan.after_slot == Some(slot_idx) {
                            for line in &orphan.lines {
                                line.write(prev_elements, &mut next_elements, false);
                            }
                        }
                    }
                }
                // And output chunk's trailing lines
                //
                // Special care is needed for the last empty line.
                // We should preserve it only if the next chunk is a boundary
                // (i.e. a `partition_by_comment` comment-only line).
                // e.g. with `partition_by_comment: true`:
                // ```text
                // import A from "a"; // chunk1
                // import B from "b"; // chunk1
                // // This empty line should be preserved because the next chunk is a boundary.
                //
                // // BOUNDARY-COMMENT
                // import C from "c"; // chunk2
                // ```
                // But in this case, we should not preserve it.
                // ```text
                // import A from "a"; // chunk1
                // import B from "b"; // chunk1
                // // This empty line should NOT be preserved because the next chunk is NOT a boundary.
                //
                // import C from "c"; // chunk2
                // ```
                let next_chunk_is_boundary =
                    chunks_iter.peek().is_some_and(|c| matches!(c, PartitionedChunk::Boundary(_)));
                for (idx, line) in trailing_lines.iter().enumerate() {
                    let is_last_empty_line =
                        idx == trailing_lines.len() - 1 && matches!(line, SourceLine::Empty);
                    line.write(
                        prev_elements,
                        &mut next_elements,
                        if is_last_empty_line { next_chunk_is_boundary } else { true },
                    );
                }
            }
        }
    }

    // The last `SourceLine::write` always pushes a trailing line break,
    // but the input chunk had none. Pop it to match the input shape.
    debug_assert!(matches!(next_elements.last(), Some(FormatElement::Line(_))));
    next_elements.pop();

    next_elements
}

/// Resolve whether a blank line should be inserted between two group indices.
/// Checks each boundary between `prev_group_idx` and `current_group_idx`,
/// using per-boundary overrides if available, otherwise the global `newlines_between`.
///
/// When groups are skipped (i.e. no imports match an intermediate group),
/// multiple boundaries are evaluated with OR semantics.
/// If any single boundary in the range resolves to `true`, a blank line is inserted.
///
/// Decreasing transitions (`prev_group_idx > current_group_idx`) can occur when
/// ignored (side-effect) imports preserve their original positions while other
/// imports are sorted. To match perfectionist's behavior, the configured boundary
/// rules are not enforced in that direction; instead, the original blank-line state
/// at this slot is preserved via `slot_had_leading_blank`.
fn should_insert_newline_between(
    global_newlines_between: bool,
    newline_boundary_overrides: &[Option<bool>],
    prev_group_idx: usize,
    current_group_idx: usize,
    slot_had_leading_blank: bool,
) -> bool {
    if prev_group_idx > current_group_idx {
        return slot_had_leading_blank;
    }

    if newline_boundary_overrides.is_empty() {
        return global_newlines_between;
    }

    for idx in prev_group_idx..current_group_idx {
        if newline_boundary_overrides.get(idx).copied().flatten().unwrap_or(global_newlines_between)
        {
            return true;
        }
    }

    false
}

// JHPATCH: Blank line rules around side effect imports (sortImports.partitionBySideEffect)
/// Resolve whether a blank line goes between two neighbors in a `partition_by_side_effect` chunk
/// when at least one of them is an ignored (side effect) import.
///
/// Blank lines are added only when any group boundary inserts newlines,
/// either through the global `newlines_between` or a `{ "newlinesBetween": true }` marker:
/// - none between two ignored imports
/// - one after a run of ignored imports
/// - one before a run of ignored imports, unless the run starts with a comment
///   (or the import before it in the source has a trailing comment)
///   and no group boundary after the previous import's group inserts newlines
fn should_insert_newline_around_side_effect(
    options: &SortImportsOptions,
    prev: &SortableImport,
    current: &SortableImport,
) -> bool {
    let has_any_newline_boundary =
        options.newlines_between || options.newline_boundary_overrides.contains(&Some(true));
    if !has_any_newline_boundary {
        return false;
    }

    match (prev.is_ignored, current.is_ignored) {
        (true, true) => false,
        (true, false) => true,
        (false, _) => {
            (current.leading_lines.is_empty() && !current.follows_trailing_comment) || {
                let boundary_count = options.groups.len().saturating_sub(1);
                (prev.group_idx..boundary_count).any(|idx| {
                    options
                        .newline_boundary_overrides
                        .get(idx)
                        .copied()
                        .flatten()
                        .unwrap_or(options.newlines_between)
                })
            }
        }
    }
}
