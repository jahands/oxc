use super::super::assert_format;

const MERGE: &str = r#"{ "sortImports": { "mergeDuplicates": true, "newlinesBetween": false } }"#;

const MERGE_SPLIT: &str = r#"{
  "sortImports": {
    "mergeDuplicates": true,
    "splitTypeSpecifiers": true,
    "sortSpecifiers": true,
    "groups": ["value-import", "type-import"]
  }
}"#;

const MERGE_SPLIT_HOIST: &str = r#"{
  "sortImports": {
    "mergeDuplicates": true,
    "splitTypeSpecifiers": true,
    "sortSpecifiers": true,
    "hoist": true,
    "groups": ["value-import", "type-import"]
  }
}"#;

#[test]
fn should_merge_imports_of_the_same_source() {
    assert_format(
        r#"
import React from "react";
import { useState } from "react";
import { b } from "zod";
import { a } from "zod";
import type { A } from "zod";
import type { B } from "zod";
"#,
        MERGE,
        r#"
import React, { useState } from "react";
import { b, a, type A, type B } from "zod";
"#,
    );
    // The default specifier comes first, whichever import has it.
    assert_format(
        r#"
import { useState } from "react";
import React from "react";
"#,
        MERGE,
        r#"
import React, { useState } from "react";
"#,
    );
    // Without the option, duplicates stay.
    assert_format(
        r#"
import { a } from "zod";
import { b } from "zod";
"#,
        r#"{ "sortImports": { "newlinesBetween": false } }"#,
        r#"
import { a } from "zod";
import { b } from "zod";
"#,
    );
}

#[test]
fn should_not_merge_imports_that_cannot_be_combined() {
    assert_format(
        r#"
import * as ns from "a";
import { a1 } from "a";
import { a2 } from "a";
import B1 from "b";
import B2 from "b";
import type T from "c";
import type { U } from "c";
import { d1 } from "d";
import "d";
import { d2 } from "d";
"#,
        MERGE,
        r#"
import * as ns from "a";
import { a1 } from "a";
import { a2 } from "a";
import B1 from "b";
import B2 from "b";
import type T from "c";
import type { U } from "c";
import { d1, d2 } from "d";
import "d";
"#,
    );
    // Imports with different import attributes stay apart.
    assert_format(
        r#"
import { a } from "./a.json" with { type: "json" };
import { b } from "./a.json" with { type: "json" };
import { c } from "./c.json" with { type: "json" };
import { d } from "./c.json";
"#,
        MERGE,
        r#"
import { a, b } from "./a.json" with { type: "json" };
import { c } from "./c.json" with { type: "json" };
import { d } from "./c.json";
"#,
    );
}

#[test]
fn should_merge_within_side_effect_partitions() {
    assert_format(
        r#"
import { a } from "x";
import "./setup";
import { b } from "x";
// prettier-ignore
import { c } from "x";
import { d } from "x";
"#,
        r#"{ "sortImports": { "mergeDuplicates": true, "partitionBySideEffect": true } }"#,
        r#"
import { a } from "x";

import "./setup";

import { b } from "x";

// prettier-ignore
import { c } from "x";

import { d } from "x";
"#,
    );
}

#[test]
fn should_merge_type_imports_into_value_imports_without_split() {
    assert_format(
        r#"
import { a } from "x";
import type { B, C } from "x";
"#,
        MERGE,
        r#"
import { a, type B, type C } from "x";
"#,
    );
    // The import that comes first is kept.
    assert_format(
        r#"
const x = 1;
import type { B } from "x";
import { a } from "x";
"#,
        MERGE,
        r#"
const x = 1;
import { type B, a } from "x";
"#,
    );
    // An import at the start of the source is merged into the later one.
    assert_format(
        r#"
import type { B } from "x";
import { a } from "x";
"#,
        MERGE,
        r#"
import { a, type B } from "x";
"#,
    );
}

#[test]
fn should_keep_comments_of_merged_imports() {
    assert_format(
        r#"
import { c } from "y";
// About a.
import { a } from "x"; // Trailing a.
// About b.
import { b } from "x"; // Trailing b.
import {
  d, // About d.
} from "x";
"#,
        MERGE,
        r#"
// About a.
// About b.
import {
  a,
  b,
  d, // About d.
} from "x"; // Trailing a.
// Trailing b.
import { c } from "y";
"#,
    );
}

#[test]
fn should_split_inline_type_specifiers() {
    assert_format(
        r#"
import { type A, b } from "x";
import D, { type E } from "y";
import { type F } from "z";
import { type G, type H } from "w";
import { type I, j } from "./data.json" with { type: "json" };
"#,
        MERGE_SPLIT,
        r#"
import { j, type I } from "./data.json" with { type: "json" };
import { type G, type H } from "w";
import { b } from "x";
import D from "y";
import { type F } from "z";

import type { A } from "x";
import type { E } from "y";
"#,
    );
    // Merged imports are split after merging.
    assert_format(
        r#"
import { a } from "x";
import { type C, b } from "x";
"#,
        MERGE_SPLIT,
        r#"
import { a, b } from "x";

import type { C } from "x";
"#,
    );
    // Without `mergeDuplicates`, imports are only split.
    assert_format(
        r#"
import { a } from "x";
import { type C, b } from "x";
"#,
        r#"{ "sortImports": { "splitTypeSpecifiers": true, "groups": ["value-import", "type-import"] } }"#,
        r#"
import { a } from "x";
import { b } from "x";

import type { C } from "x";
"#,
    );
}

#[test]
fn should_place_comments_like_prettier_with_hoist() {
    // The comments of an import merged into the first import of the file stay with it.
    assert_format(
        r#"
// Header.
import { a } from "x";
import { c } from "q";
// About b.
import { b } from "x";
import { d } from "r";
"#,
        MERGE_SPLIT_HOIST,
        r#"
// Header.
import { c } from "q";
import { d } from "r";
// About b.
import { a, b } from "x";
"#,
    );
    // The comments of a split import go below the imports.
    assert_format(
        r#"
import { c } from "y";
// About x.
import { type A, b } from "x"; // Trailing x.
const q = 1;
"#,
        MERGE_SPLIT_HOIST,
        r#"
import { b } from "x";
import { c } from "y";

import type { A } from "x";

// About x.
// Trailing x.
const q = 1;
"#,
    );
    // The trailing comment of a merged import goes to the next import, or below the imports.
    assert_format(
        r#"
import { c } from "y";
import { a } from "x";
import { b } from "x"; // Trailing b.
import { e } from "w";
const q = 1;
import { d } from "x"; // Trailing d.
f();
"#,
        MERGE_SPLIT_HOIST,
        r#"
// Trailing b.
import { e } from "w";
import { a, b, d } from "x";
import { c } from "y";

const q = 1;
// Trailing d.
f();
"#,
    );
}

#[test]
fn should_place_specifier_comments_like_prettier() {
    // A comment on its own line at the end of the list stays at the end.
    assert_format(
        r#"
import { a } from "x";
import {
  c,
  b,
  // d,
} from "x";
"#,
        MERGE_SPLIT,
        r#"
import {
  a,
  b,
  c,
  // d,
} from "x";
"#,
    );
    // A comment after `{` on the line of the default specifier goes after it.
    assert_format(
        r#"
import D, { // About D.
  a,
} from "x";
import { b } from "x";
"#,
        MERGE_SPLIT,
        r#"
import D, { a, b } from "x"; // About D.
"#,
    );
    // A block comment on the line of a specifier stays on it.
    assert_format(
        r#"
import {
  /* About T. */ type T,
  v,
  w,
} from "x";
"#,
        MERGE_SPLIT,
        r#"
import { v, w } from "x";

import type { /* About T. */ T } from "x";
"#,
    );
    // A block comment after a comma leads the next specifier.
    assert_format(
        r#"
import { type T, /* About v. */ v } from "x";
"#,
        MERGE_SPLIT,
        r#"
import { /* About v. */ v } from "x";

import type { T } from "x";
"#,
    );
    // A single-line block comment on the line above a specifier goes after the specifier before it.
    assert_format(
        r#"
import { a } from "x";
import {
  /* About c. */
  c,
  b,
} from "x";
"#,
        MERGE_SPLIT,
        r#"
import { a, b /* About c. */, c } from "x";
"#,
    );
}

#[test]
fn should_print_the_first_import_on_the_line_of_a_moved_block_comment() {
    assert_format(
        r#"
import { a } from "x";
/**
 * About b.
 */
import { b } from "x";
"#,
        MERGE_SPLIT_HOIST,
        r#"
/**
 * About b.
 */ import { a, b } from "x";
"#,
    );
    assert_format(
        r#"
import { z } from "z";
import { a } from "x";
/* About
 * b. */
import { b } from "x";
"#,
        MERGE_SPLIT_HOIST,
        r#"
/* About
 * b. */ import { a, b } from "x";
import { z } from "z";
"#,
    );
    // An import printed below another one stays below the comment.
    assert_format(
        r#"
import { a } from "a";
import { b } from "x";
/* About
 * c. */
import { c } from "x";
"#,
        MERGE_SPLIT_HOIST,
        r#"
import { a } from "a";
/* About
 * c. */
import { b, c } from "x";
"#,
    );
}
