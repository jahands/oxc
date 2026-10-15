use oxc_formatter_core::format_element::LineMode;

use crate::ir_transform::sort_imports::{
    compute_metadata::compute_import_metadata,
    group_matcher::GroupMatcher,
    options::SortImportsOptions,
    sortable_imports::{SortSortableImports, SortableImport},
    source_line::SourceLine,
};

/// Orphan content (comments/empty lines separated by empty line from the next import).
/// These stay at their original slot position after sorting.
#[derive(Debug)]
pub struct OrphanContent<'a> {
    pub lines: Vec<SourceLine<'a>>,
    /// The slot position:
    /// - `None`: before the first import (leading orphan)
    /// - `Some(n)`: after the Nth import (0-indexed)
    pub after_slot: Option<usize>,
}

#[derive(Debug)]
pub enum PartitionedChunk<'a> {
    /// A chunk containing import statements,
    /// and possibly leading/trailing comments or empty lines.
    Imports(Vec<SourceLine<'a>>),
    /// A boundary chunk.
    /// Contains `SourceLine::Empty` (when `partition_by_newline` is true)
    /// or `SourceLine::CommentOnly` (when `partition_by_comment` is true).
    Boundary(SourceLine<'a>),
}

impl Default for PartitionedChunk<'_> {
    fn default() -> Self {
        Self::Imports(vec![])
    }
}

impl<'a> PartitionedChunk<'a> {
    pub fn add_imports_line(&mut self, line: SourceLine<'a>) {
        match self {
            Self::Imports(lines) => lines.push(line),
            Self::Boundary(_) => {
                unreachable!("Cannot add to a boundary chunk");
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Imports(lines) if lines.is_empty())
    }

    /// Convert this import chunk into `SortableImport` units with `OrphanContent`.
    /// Returns a tuple of `(sortable_imports, orphan_contents, trailing_lines, slot_had_leading_blank)`.
    ///
    /// - `sortable_imports`: Import statements with their attached leading lines.
    ///   - `leading_lines`: Comments directly before this import (no empty line between).
    /// - `orphan_contents`: Orphan comments (separated by empty line from next import) with their slot positions.
    ///   - `after_slot: None` = before first import, `Some(n)` = after slot n
    /// - `trailing_lines`: Lines at the end of the chunk after all imports.
    /// - `slot_had_leading_blank`: For each slot index, whether the original input had
    ///   at least one blank line before that slot. Indexed by slot position
    ///   (which is preserved across sorting), not by the import currently at that slot.
    // JHPATCH: Document keep_leading_comments_at_top (sortImports.hoist)
    ///
    /// With `keep_leading_comments_at_top`, the comments directly before the first import
    /// become leading orphan content instead of moving with that import.
    #[must_use]
    pub fn into_sorted_import_units(
        self,
        group_matcher: &GroupMatcher,
        options: &SortImportsOptions,
        // JHPATCH: Keep the comments before the first import at the top (sortImports.hoist)
        keep_leading_comments_at_top: bool,
    ) -> (Vec<SortableImport<'a>>, Vec<OrphanContent<'a>>, Vec<SourceLine<'a>>, Vec<bool>) {
        let Self::Imports(lines) = self else {
            unreachable!(
                "`into_import_units()` must be called on `PartitionedChunk::Imports` only."
            );
        };

        let mut sortable_imports: Vec<SortableImport<'a>> = Vec::with_capacity(lines.len());
        let mut orphan_contents: Vec<OrphanContent<'a>> = vec![];
        let mut slot_had_leading_blank: Vec<bool> = Vec::with_capacity(lines.len());

        // Comments separated from the next import by empty line.
        // These stay at their slot position, not attached to any import.
        let mut orphan_pending: Vec<SourceLine<'a>> = vec![];
        // Comments directly before the next import (no empty line between).
        // These attach to the next import as leading lines.
        let mut current_pending: Vec<SourceLine<'a>> = vec![];

        // JHPATCH: Record the source line index of each import (sortImports.hoist)
        for (line_idx, line) in lines.into_iter().enumerate() {
            match line {
                SourceLine::Import(_, ref metadata) => {
                    // A blank line in the original source ends up in `orphan_pending`,
                    // so its presence there marks this slot as having a leading blank.
                    slot_had_leading_blank
                        .push(orphan_pending.iter().any(|l| matches!(l, SourceLine::Empty)));
                    // JHPATCH: Keep the comments before the first import at the top (sortImports.hoist)
                    if keep_leading_comments_at_top && sortable_imports.is_empty() {
                        // JHPATCH: Comments that move with the first import (sortImports.mergeDuplicates)
                        // The comments that hoist_imports moved above the first import move with it.
                        let mut seen = 0;
                        let split_at = (metadata.moving_comments > 0)
                            .then(|| {
                                current_pending.iter().rposition(|line| {
                                    let is_comment = matches!(line, SourceLine::CommentOnly(..));
                                    seen += usize::from(is_comment);
                                    is_comment && seen == metadata.moving_comments
                                })
                            })
                            .flatten()
                            .unwrap_or(current_pending.len());
                        let mut moved = current_pending.split_off(split_at);
                        orphan_pending.append(&mut current_pending);
                        current_pending.append(&mut moved);
                    }

                    // Handle orphan content (separated by empty line from this import)
                    // These stay at their original slot position, not attached to any import.
                    if !orphan_pending.is_empty() {
                        // Slot position: None = before first import, Some(n) = after slot n
                        let after_slot = if sortable_imports.is_empty() {
                            None
                        } else {
                            Some(sortable_imports.len() - 1)
                        };
                        // For leading orphans (None), keep all lines including empty lines
                        // For other orphans, keep only comments
                        let lines: Vec<_> = if after_slot.is_none() {
                            std::mem::take(&mut orphan_pending)
                        } else {
                            std::mem::take(&mut orphan_pending)
                                .into_iter()
                                .filter_map(|orphan| {
                                    if let SourceLine::CommentOnly(range, _) = orphan {
                                        Some(SourceLine::CommentOnly(range, LineMode::Hard))
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        };
                        if !lines.is_empty() {
                            orphan_contents.push(OrphanContent { lines, after_slot });
                        }
                    }

                    let is_side_effect = metadata.is_side_effect;
                    // JHPATCH: Trailing comment info for hoist and side effect blank lines
                    let has_trailing_comment = metadata.has_trailing_comment;
                    // JHPATCH: The trailing comment of an import that is not the previous statement does not count (sortImports.hoist)
                    let follows_trailing_comment = !metadata.is_detached
                        && sortable_imports.last().is_some_and(|prev| prev.has_trailing_comment);
                    // JHPATCH: Rewritten import metadata (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
                    let moving_comments = metadata.moving_comments;
                    let is_split = metadata.is_split;
                    let (group_idx, normalized_source, is_ignored) =
                        compute_import_metadata(metadata, group_matcher, options);

                    sortable_imports.push(SortableImport {
                        leading_lines: std::mem::take(&mut current_pending),
                        import_line: line,
                        // JHPATCH: Line and trailing comment info for hoist and side effect blank lines
                        line_idx,
                        has_trailing_comment,
                        follows_trailing_comment,
                        // JHPATCH: Rewritten import metadata (sortImports.hoist, sortImports.mergeDuplicates, sortImports.splitTypeSpecifiers)
                        moving_comments,
                        is_split,
                        is_side_effect,
                        group_idx,
                        normalized_source,
                        is_ignored,
                    });
                }
                // JHPATCH: Comments separated by blank lines move with the import (sortImports.hoist)
                // With `hoist`, every comment before an import moves with it,
                // keeping the empty lines that follow the first comment.
                SourceLine::Empty if options.hoist && !current_pending.is_empty() => {
                    current_pending.push(line);
                }
                SourceLine::Empty => {
                    // Empty line separates comments from the next import.
                    // Move `current_pending` to `orphan_pending`, then add empty line.
                    orphan_pending.append(&mut current_pending);
                    orphan_pending.push(line);
                }
                SourceLine::CommentOnly(..) => {
                    current_pending.push(line);
                }
            }
        }

        // Any remaining lines are trailing
        // Combine orphan and current pending as trailing lines
        orphan_pending.append(&mut current_pending);
        let trailing_lines = orphan_pending;

        // Let's sort this chunk!
        sortable_imports.sort(options);

        (sortable_imports, orphan_contents, trailing_lines, slot_had_leading_blank)
    }
}
