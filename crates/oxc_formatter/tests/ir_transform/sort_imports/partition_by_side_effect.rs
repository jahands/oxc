use super::super::assert_format;

#[test]
fn should_not_sort_across_side_effect_imports() {
    assert_format(
        r#"
import { z } from "z";
import { y } from "y";
import "./setup";
import { b } from "b";
import { a } from "a";
"#,
        r#"{ "sortImports": { "partitionBySideEffect": true, "newlinesBetween": false } }"#,
        r#"
import { y } from "y";
import { z } from "z";
import "./setup";
import { a } from "a";
import { b } from "b";
"#,
    );
    // Without the option, the side effect import keeps its slot and the others are sorted across it.
    assert_format(
        r#"
import { z } from "z";
import { y } from "y";
import "./setup";
import { b } from "b";
import { a } from "a";
"#,
        r#"{ "sortImports": { "newlinesBetween": false } }"#,
        r#"
import { a } from "a";
import { b } from "b";
import "./setup";
import { y } from "y";
import { z } from "z";
"#,
    );
}

#[test]
fn should_add_blank_lines_around_side_effect_runs() {
    assert_format(
        r#"
import "zx/globals";
import { program } from "commander";
import { b } from "b";
import "./polyfill";
import "./setup";
import { a } from "a";
"#,
        r#"{ "sortImports": { "partitionBySideEffect": true } }"#,
        r#"
import "zx/globals";

import { b } from "b";
import { program } from "commander";

import "./polyfill";
import "./setup";

import { a } from "a";
"#,
    );
    // A `{ "newlinesBetween": true }` marker anywhere enables the blank lines.
    assert_format(
        r#"
import { b } from "b";
import "./setup";
import { a } from "a";
"#,
        r#"{
            "sortImports": {
                "partitionBySideEffect": true,
                "newlinesBetween": false,
                "groups": ["value-external", { "newlinesBetween": true }, "unknown"]
            }
        }"#,
        r#"
import { b } from "b";

import "./setup";

import { a } from "a";
"#,
    );
}

#[test]
fn should_skip_blank_line_before_commented_side_effect_after_group_without_newline() {
    // `type-import` is the last group, so no boundary after it inserts newlines.
    let config = r#"{
        "sortImports": {
            "partitionBySideEffect": true,
            "newlinesBetween": false,
            "groups": ["value-external", { "newlinesBetween": true }, "type-import"]
        }
    }"#;
    assert_format(
        r#"
import type { T } from "t";
// leading comment
import "./side";
import { a } from "a";
import "./side2";
import type { U } from "u";
"#,
        config,
        r#"
import type { T } from "t";
// leading comment
import "./side";

import { a } from "a";

import "./side2";

import type { U } from "u";
"#,
    );
    // After `value-external`, the boundary to `type-import` inserts newlines.
    assert_format(
        r#"
import { a } from "a";
// leading comment
import "./side";
"#,
        config,
        r#"
import { a } from "a";

// leading comment
import "./side";
"#,
    );
}

#[test]
fn should_have_no_effect_when_side_effects_are_sorted() {
    assert_format(
        r#"
import { b } from "b";
import "./setup";
import { a } from "a";
"#,
        r#"{
            "sortImports": {
                "partitionBySideEffect": true,
                "sortSideEffects": true,
                "newlinesBetween": false
            }
        }"#,
        r#"
import { a } from "a";
import { b } from "b";
import "./setup";
"#,
    );
}
