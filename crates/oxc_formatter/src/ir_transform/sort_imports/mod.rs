// JHPATCH: Intl collation module (sortImports.collation)
pub mod collation;
mod compute_metadata;
mod group_config;
mod group_matcher;
// JHPATCH: Import hoisting module (sortImports.hoist)
mod hoist;
// JHPATCH: Import merging and splitting module (sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
mod merge;
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
pub use hoist::{HoistFacts, HoistedImportFacts, ImportRewriteInfo, hoist_imports};
// JHPATCH: Import merging and splitting entry points (sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
pub use merge::{apply_import_rewrites, plan_import_rewrites};

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
// JHPATCH: Pass where the chunk sits in the source for the hoist comment rules (sortImports.hoist)
pub fn sort_imports_chunk(
    formatter: &mut JsFormatter<'_, '_>,
    chunk_start: usize,
    source: &ChunkSource,
    // JHPATCH: Start offsets of the imports in the chunk, to look up rewritten imports (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
    import_starts: &[u32],
) {
    let elements = &formatter.elements()[chunk_start..];
    let options = formatter.options().sort_imports.as_ref().unwrap();
    // JHPATCH: Rewritten import metadata (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
    let info = formatter.context().import_rewrite_info();
    let rewritten: Vec<RewrittenImport> = import_starts
        .iter()
        .map(|start| RewrittenImport {
            moving_comments: info
                .moving_comments
                .iter()
                .find(|(moving_start, _)| moving_start == start)
                .map_or(0, |(_, count)| *count),
            is_split: info.split_import_starts.contains(start),
            joins_comment_when_first: info.joins_comment_when_first.contains(start),
        })
        .collect();

    let sorted_elements = transform(elements, options, formatter.allocator(), source, &rewritten);
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

// JHPATCH: Where a chunk sits in the source, for the hoist comment rules (sortImports.hoist)
/// Where a chunk of `ImportDeclaration`s sits in the source, for the comment and blank line rules of `hoist`.
#[derive(Debug)]
pub struct ChunkSource {
    pub position: ChunkPosition,
    /// The source lines of each import of the chunk, in source order. Empty without `hoist`.
    pub imports: Vec<ImportLines>,
    /// Whether the first import had "top of file" comments before `hoist` moved it,
    /// when `hoist` rewrote the source. See [`HoistFacts::has_top_of_file_comments`].
    pub has_top_of_file_comments: Option<bool>,
    /// See [`HoistFacts::top_comment_gap`]. 1 without `hoist`.
    pub top_comment_gap: u32,
    /// See [`HoistFacts::blank_before_first_import_comments`].
    pub blank_before_first_import_comments: bool,
    /// See [`HoistFacts::first_import_comment_height`].
    pub first_import_comment_height: u32,
}

// JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
/// Where an import of a chunk starts in the source, for the blank line rules of `hoist`.
#[derive(Clone, Copy, Debug)]
pub struct ImportLines {
    /// Line of the import declaration.
    pub line: u32,
    /// Lines from the start of the comments that the import owns to the import declaration.
    pub comment_height: u32,
    /// Whether the previous statement in the source before `hoist` was an import.
    pub follows_import: bool,
    /// Whether a comment starts after the import declaration on the line where it starts.
    pub has_trailing_comment: bool,
    /// Lines from the import declaration to the end of the last comment that starts after it on its line.
    pub trailing_comment_height: u32,
    /// Whether the import keeps its position like a side effect import.
    pub keeps_position: bool,
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
    // JHPATCH: Where the chunk sits in the source, for the hoist comment rules (sortImports.hoist)
    source: &ChunkSource,
    // JHPATCH: Rewritten import metadata (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
    rewritten: &[RewrittenImport],
) -> ArenaVec<'a, FormatElement<'a>> {
    let position = source.position;
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
                    // JHPATCH: Keep comments that follow an own-line trailing comment on its line (sortImports.hoist)
                    let continues_line = matches!(
                        prev_elements.get(idx + 1),
                        Some(FormatElement::Tag(Tag::StartLineSuffix))
                    ) && !matches!(
                        prev_elements.get(idx + 2),
                        Some(FormatElement::Line(LineMode::Hard | LineMode::Empty))
                    );
                    if inside_own_line_suffix && !continues_line {
                        inside_own_line_suffix = false;
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

    // JHPATCH: Rewritten import metadata (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
    let import_metadata = lines.iter_mut().filter_map(|line| match line {
        SourceLine::Import(_, metadata) => Some(metadata),
        _ => None,
    });
    for (metadata, rewritten) in import_metadata.zip(rewritten) {
        metadata.moving_comments = rewritten.moving_comments;
        metadata.is_split = rewritten.is_split;
        metadata.joins_comment_when_first = rewritten.joins_comment_when_first;
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
    // JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
    let mut imports_seen = 0;
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
                        source.imports.get(imports_seen..).unwrap_or_default(),
                    );
                // JHPATCH: Source lines of the imports for the hoist blank line rules (sortImports.hoist)
                imports_seen += sorted_imports.len();

                // Output leading orphan content (after_slot: None)
                // JHPATCH: Track comments kept at the top (sortImports.hoist)
                let mut leading_orphan_ends_with_comment = false;
                let mut has_top_comments = false;
                // JHPATCH: Split imports have no source line (sortImports.splitTypeSpecifiers)
                // An import split from a value import has no line in the source,
                // so no blank line separates it from the comments kept at the top.
                let first_is_split =
                    options.hoist && sorted_imports.first().is_some_and(|first| first.is_split);
                // JHPATCH: Line of the first import in the source (sortImports.hoist)
                let first_source_line =
                    sorted_imports.iter().map(|import| import.line).min().unwrap_or_default();
                // JHPATCH: Blank line before the comments of the first import after a prologue (sortImports.hoist)
                // They are the last line kept at the top, and get the blank line
                // only when that import is still sorted first.
                let blank_before_last_orphan_line = options.hoist
                    && position == ChunkPosition::AfterPrologue
                    && source.blank_before_first_import_comments
                    && sorted_imports.first().is_some_and(|first| first.line == first_source_line);
                for orphan in &orphan_contents {
                    if orphan.after_slot.is_none() {
                        let lines = if first_is_split {
                            let end = orphan
                                .lines
                                .iter()
                                .rposition(|line| !matches!(line, SourceLine::Empty))
                                .map_or(0, |idx| idx + 1);
                            &orphan.lines[..end]
                        } else {
                            &orphan.lines[..]
                        };
                        for (idx, line) in lines.iter().enumerate() {
                            // JHPATCH: Blank line before the comments of the first import after a prologue (sortImports.hoist)
                            if blank_before_last_orphan_line
                                && idx > 0
                                && idx + 1 == lines.len()
                                && matches!(lines[idx - 1], SourceLine::CommentOnly(..))
                            {
                                next_elements.push(FormatElement::Line(LineMode::Empty));
                            }
                            line.write(prev_elements, &mut next_elements, true);
                        }
                        // JHPATCH: Track comments kept at the top (sortImports.hoist)
                        leading_orphan_ends_with_comment =
                            matches!(orphan.lines.last(), Some(SourceLine::CommentOnly(..)));
                        has_top_comments = options.hoist
                            && source.has_top_of_file_comments.unwrap_or(true)
                            && orphan
                                .lines
                                .iter()
                                .any(|line| matches!(line, SourceLine::CommentOnly(..)));
                    }
                }
                // JHPATCH: Blank line between top comments and the first import's comments (sortImports.hoist)
                // With `hoist`, comments kept at the top of the file are separated by a blank line
                // from the import sorted to the front when that import, moved to the line of the first
                // import in the source below its own comments, starts two or more lines below them.
                // JHPATCH: Only own comments count for the blank line below the top comments (sortImports.mergeDuplicates)
                // The comments that hoist_imports moved above an import are not its own comments.
                if has_top_comments
                    && position == ChunkPosition::FileStart
                    && leading_orphan_ends_with_comment
                    && sorted_imports.first().is_some_and(|first| {
                        let comment_lines = first
                            .leading_lines
                            .iter()
                            .filter(|line| matches!(line, SourceLine::CommentOnly(..)))
                            .count();
                        let comment_height = if first.moving_comments == 0
                            || comment_lines > first.moving_comments
                        {
                            first.comment_height
                        } else {
                            0
                        };
                        source.top_comment_gap + comment_height >= 2
                    })
                {
                    next_elements.push(FormatElement::Line(LineMode::Empty));
                }
                // Below comments kept at the top, the first import counts as starting
                // on the line of the first import in the source, after its own leading lines.
                let mut prev_line = 0;

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
                    // when the next import did not start on the line right after the end of that comment.
                    // JHPATCH: Split imports have no source line (sortImports.splitTypeSpecifiers)
                    // An import split from a value import has no line in the source, so it gets none.
                    let insert_newline = insert_newline
                        || (options.hoist
                            && !sorted_import.is_split
                            && prev_import.is_some_and(|prev| {
                                sorted_import.line > prev_line + 1 && prev.has_trailing_comment
                            }));
                    prev_line = sorted_import.trailing_comment_height
                        + if slot_idx == 0 && has_top_comments {
                            let prologue_comment_height = if sorted_import.line == first_source_line
                            {
                                source.first_import_comment_height
                            } else {
                                0
                            };
                            first_source_line
                                + sorted_import.comment_height
                                + prologue_comment_height
                        } else {
                            sorted_import.line
                        };
                    if insert_newline {
                        next_elements.push(FormatElement::Line(LineMode::Empty));
                    }
                    // JHPATCH: Line break between an import and the comments before it on its line (sortImports.hoist)
                    // With `hoist`, an import goes on its own line when its first leading comment is
                    // a line comment or spans lines, unless nothing is written before the import.
                    let breaks_before_import = options.hoist
                        && sorted_import.has_leading_comment
                        && !(position == ChunkPosition::FileStart && next_elements.is_empty())
                        && sorted_import
                            .leading_lines
                            .first()
                            .unwrap_or(&sorted_import.import_line)
                            .first_comment_ends_line(prev_elements);
                    // JHPATCH: Track the previous import for side effect and hoist blank lines
                    prev_import = Some(sorted_import);
                    if !sorted_import.is_ignored {
                        seen_non_ignored = true;
                    }

                    // JHPATCH: Whether the import goes on the line of the comment above it (sortImports.mergeDuplicates)
                    let joins_comment = sorted_import.joins_comment_when_first
                        && slot_idx == 0
                        && position == ChunkPosition::FileStart
                        && next_elements.is_empty()
                        && matches!(
                            sorted_import.leading_lines.last(),
                            Some(SourceLine::CommentOnly(..))
                        );
                    // Output leading lines and import line
                    for line in &sorted_import.leading_lines {
                        line.write(
                            prev_elements,
                            &mut next_elements,
                            // JHPATCH: Keep blank lines in leading comments (sortImports.hoist)
                            options.partition_by_newline || options.hoist,
                        );
                    }
                    // JHPATCH: Print the import on the line of the comment above it (sortImports.mergeDuplicates)
                    if joins_comment {
                        next_elements.pop();
                        next_elements.push(FormatElement::Space);
                    }
                    // JHPATCH: Line break between an import and the comments before it on its line (sortImports.hoist)
                    if breaks_before_import {
                        sorted_import
                            .import_line
                            .write_import_on_own_line(prev_elements, &mut next_elements);
                    } else {
                        sorted_import.import_line.write(prev_elements, &mut next_elements, false);
                    }

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

// JHPATCH: What the rewrite before formatting tells about an import (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
/// What [`hoist_imports`] tells about an import of a chunk.
struct RewrittenImport {
    /// Number of comments on lines of their own directly above the import that move with it
    /// when it is the first import ([`ImportRewriteInfo::moving_comments`]).
    moving_comments: usize,
    /// Whether the import was split from a value import with inline `type` specifiers.
    is_split: bool,
    /// See [`ImportRewriteInfo::joins_comment_when_first`].
    joins_comment_when_first: bool,
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
            (current.leading_lines.is_empty()
                && !current.has_leading_comment
                && !current.follows_trailing_comment)
                || {
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
