use std::ops::Range;

use oxc_allocator::ArenaVec;
use oxc_formatter_core::format_element::{
    FormatElement, LineMode,
    tag::{LabelId, Tag},
};

use crate::JsLabels;

#[derive(Debug)]
pub enum SourceLine<'a> {
    /// Line that contains an import statement.
    /// May have leading comments like `/* ... */ import ...`.
    /// And also may have trailing comments like `import ...; // ...`.
    /// Never be a boundary.
    Import(Range<usize>, ImportLineMetadata<'a>),
    /// Empty line.
    /// May be used as a boundary if `options.partition_by_newline` is true.
    Empty,
    /// Line that contains only comment(s).
    /// May be used as a boundary if `options.partition_by_comment` is true.
    CommentOnly(Range<usize>, LineMode),
}

impl<'a> SourceLine<'a> {
    pub fn from_element_range(
        elements: &[FormatElement<'a>],
        range: Range<usize>,
        line_mode: LineMode,
    ) -> Self {
        debug_assert!(
            !range.is_empty(),
            "`range` must not be empty, otherwise use `SourceLine::Empty` directly."
        );

        // The chunk only contains:
        // - non-suppressed `ImportDeclaration`s (wrapped with `JsLabels::ImportDeclaration`)
        // - and their surrounding comments / line breaks
        // So the label's presence is a sufficient signal for `Import` vs `CommentOnly`.
        // Textual prefix checks would be fragile against.
        // (e.g. formatted JSDoc, which splits a single comment into a mix of `Token`/`Text` elements.)
        let mut has_import = false;
        let mut source = None;
        let mut is_type_import = false;
        let mut has_default_specifier = false;
        let mut has_namespace_specifier = false;
        let mut has_named_specifier = false;

        // One-way scan state, bounded by `range`.
        // No look-ahead: looking ahead past `range` would inherit the NEXT import's specifiers.
        // Seen the `import` keyword; specifier detection is active from here.
        let mut seen_import_keyword = false;
        // Only the token right after `import` can be the `type` keyword
        // (`type` inside `{ ... }` is an inline type specifier, not a type import).
        let mut expect_type_keyword = false;
        // Seen the `from` keyword; everything after is the source and import attributes
        // (`with { type: "json" }` must not count as named specifiers).
        let mut seen_from_keyword = false;

        let import_label = LabelId::of(JsLabels::ImportDeclaration);
        // JHPATCH: Read suppressed imports as imports that keep their position (sortImports.partitionBySideEffect)
        let suppressed_import_label = LabelId::of(JsLabels::SuppressedImportDeclaration);
        for idx in range.clone() {
            let element = &elements[idx];

            // Special marker for `ImportDeclaration`
            if let FormatElement::Tag(Tag::StartLabelled(id)) = element {
                if *id == import_label {
                    has_import = true;
                // JHPATCH: Suppressed import line metadata (sortImports.partitionBySideEffect)
                } else if *id == suppressed_import_label {
                    return SourceLine::Import(
                        range.clone(),
                        ImportLineMetadata {
                            source: "",
                            is_suppressed: true,
                            has_trailing_comment: has_comment_after_import(elements, range),
                            is_side_effect: false,
                            is_type_import: false,
                            has_default_specifier: false,
                            has_namespace_specifier: false,
                            has_named_specifier: false,
                        },
                    );
                }
                continue;
            }
            if !has_import {
                continue;
            }

            // Everything before the `import` keyword (e.g. leading comments) is not the head.
            if !seen_import_keyword {
                if matches!(element, FormatElement::Token { text: "import" }) {
                    seen_import_keyword = true;
                    expect_type_keyword = true;
                }
                continue;
            }

            match element {
                FormatElement::Token { text } => {
                    if !seen_from_keyword {
                        match *text {
                            "type" if expect_type_keyword => is_type_import = true,
                            "*" => has_namespace_specifier = true,
                            "{" => has_named_specifier = true,
                            "from" => {
                                seen_from_keyword = true;
                                source = None;
                            }
                            _ => {}
                        }
                    }
                    expect_type_keyword = false;
                }
                FormatElement::Text { text, .. } => {
                    if source.is_none() {
                        source = Some(text);
                        // The first `Text` after `from` is the source;
                        // nothing after it (import attributes, trailing comments) affects the metadata.
                        if seen_from_keyword {
                            break;
                        }
                    }
                    // A bare identifier in the head before `{` / `*` is the default binding.
                    // For a side-effect import this same position holds the module source;
                    // that artifact is cleared below once no `from` shows up.
                    if !has_namespace_specifier && !has_named_specifier {
                        has_default_specifier = true;
                    }
                    expect_type_keyword = false;
                }
                // Spaces (and line breaks inside a multiline import) separate head tokens
                // without closing the `type` keyword window.
                _ => {}
            }
        }

        if !has_import {
            return SourceLine::CommentOnly(range, line_mode);
        }

        // No `from` means a genuine side-effect import, which has no bindings at all:
        // whatever the head scan collected (its own source `Text` as a default binding,
        // attribute braces as named specifiers) is an artifact.
        let is_side_effect = !seen_from_keyword;
        if is_side_effect {
            has_default_specifier = false;
            has_namespace_specifier = false;
            has_named_specifier = false;
        }

        SourceLine::Import(
            // JHPATCH: Suppression and trailing comment metadata (sortImports.partitionBySideEffect, sortImports.hoist)
            range.clone(),
            ImportLineMetadata {
                source: source.expect("`ImportDeclaration` must have a source"),
                is_suppressed: false,
                has_trailing_comment: has_comment_after_import(elements, range),
                is_side_effect,
                is_type_import,
                has_default_specifier,
                has_namespace_specifier,
                has_named_specifier,
            },
        )
    }

    pub fn write(
        &self,
        prev_elements: &[FormatElement<'a>],
        next_elements: &mut ArenaVec<'a, FormatElement<'a>>,
        preserve_empty_line: bool,
    ) {
        match self {
            SourceLine::Empty => {
                // Skip empty lines unless they should be preserved
                if preserve_empty_line {
                    next_elements.push(FormatElement::Line(LineMode::Empty));
                }
            }
            SourceLine::Import(range, _) => {
                for idx in range.clone() {
                    next_elements.push(prev_elements[idx].clone());
                }
                // Always use hard line break after import statement.
                next_elements.push(FormatElement::Line(LineMode::Hard));
            }
            SourceLine::CommentOnly(range, mode) => {
                for idx in range.clone() {
                    next_elements.push(prev_elements[idx].clone());
                }
                next_elements.push(FormatElement::Line(*mode));
            }
        }
    }
}

/// Import line metadata extracted during parsing.
/// Just holds the information found, without interpretation.
#[derive(Debug)]
pub struct ImportLineMetadata<'a> {
    /// Index of the import source in the original `elements` slice.
    pub source: &'a str,
    // JHPATCH: Suppression and trailing comment metadata (sortImports.partitionBySideEffect, sortImports.hoist)
    /// Whether this import is suppressed (`prettier-ignore` / `oxfmt-ignore`) and printed verbatim.
    /// The fields other than `has_trailing_comment` are not computed for it.
    pub is_suppressed: bool,
    /// Whether a comment follows the import on its line.
    pub has_trailing_comment: bool,
    /// Whether this is a side-effect-only import (e.g., `import "foo"`).
    pub is_side_effect: bool,
    /// Whether this is a type-only import (e.g., `import type { Foo } from "foo"`).
    pub is_type_import: bool,
    /// Whether this import has a default specifier (e.g., `import Foo from "foo"`).
    pub has_default_specifier: bool,
    /// Whether this import has a namespace specifier (e.g., `import * as Foo from "foo"`).
    pub has_namespace_specifier: bool,
    /// Whether this import has named specifiers (e.g., `import { foo } from "foo"`).
    pub has_named_specifier: bool,
}

// JHPATCH: Trailing comment detection (sortImports.partitionBySideEffect, sortImports.hoist)
/// Whether a comment follows the import in `elements[range]`, a line that holds an import.
fn has_comment_after_import(elements: &[FormatElement<'_>], range: Range<usize>) -> bool {
    let import_label = LabelId::of(JsLabels::ImportDeclaration);
    let suppressed_import_label = LabelId::of(JsLabels::SuppressedImportDeclaration);
    let comment_label = LabelId::of(JsLabels::Comment);
    let mut open_labels = vec![];
    let mut after_import = false;
    for element in &elements[range] {
        match element {
            FormatElement::Tag(Tag::StartLabelled(id)) => {
                if after_import && *id == comment_label {
                    return true;
                }
                open_labels.push(*id);
            }
            FormatElement::Tag(Tag::EndLabelled)
                if open_labels
                    .pop()
                    .is_some_and(|id| id == import_label || id == suppressed_import_label) =>
            {
                after_import = true;
            }
            _ => {}
        }
    }
    false
}
