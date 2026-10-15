use super::super::assert_format;

const HOIST: &str = r#"{ "sortImports": { "hoist": true, "newlinesBetween": false } }"#;

#[test]
fn should_move_imports_after_code_to_the_top() {
    assert_format(
        r#"
import { b } from "b";
const x = 1;
// About `a`.
import { a } from "a";
const y = 2;
"#,
        HOIST,
        r#"
// About `a`.
import { a } from "a";
import { b } from "b";

const x = 1;

const y = 2;
"#,
    );
    assert_format(
        r#"
const x = 1;
import { b } from "b";
function f() {
  return import("c");
}
import { a } from "a"; // Trailing `a`.
f();
"#,
        HOIST,
        r#"
import { a } from "a"; // Trailing `a`.
import { b } from "b";

const x = 1;

function f() {
  return import("c");
}

f();
"#,
    );
    // Without the option, imports after code stay in place.
    assert_format(
        r#"
import { b } from "b";
const x = 1;
import { a } from "a";
"#,
        r#"{ "sortImports": { "newlinesBetween": false } }"#,
        r#"
import { b } from "b";
const x = 1;
import { a } from "a";
"#,
    );
}

#[test]
fn should_leave_exports_and_nested_imports_in_place() {
    assert_format(
        r#"
import { a } from "a";
export * from "./x";
export { y } from "./y";
import { b } from "b";
declare module "m" {
  import { d } from "d";
  import { c } from "c";
}
"#,
        HOIST,
        r#"
import { a } from "a";
import { b } from "b";

export * from "./x";
export { y } from "./y";

declare module "m" {
  import { d } from "d";
  import { c } from "c";
}
"#,
    );
}

#[test]
fn should_add_blank_line_after_imports() {
    assert_format(
        r#"
import { a } from "a";
export const x = 1;
"#,
        HOIST,
        r#"
import { a } from "a";

export const x = 1;
"#,
    );
    assert_format(
        r#"
import { a } from "a"; // Trailing `a`.
// About `x`.
const x = 1;
"#,
        HOIST,
        r#"
import { a } from "a"; // Trailing `a`.

// About `x`.
const x = 1;
"#,
    );
    assert_format(
        r#"
import { b } from "b";
import { a } from "a";
// Last comment.
"#,
        HOIST,
        r#"
import { a } from "a";
import { b } from "b";

// Last comment.
"#,
    );
    assert_format(
        r#"
import { a } from "a";
// Last comment.
"#,
        HOIST,
        r#"
import { a } from "a";

// Last comment.
"#,
    );
}

#[test]
fn should_place_imports_after_hashbang_and_directives() {
    assert_format(
        r#"
#!/usr/bin/env node

import { a } from "a";
"#,
        HOIST,
        r#"
#!/usr/bin/env node
import { a } from "a";
"#,
    );
    assert_format(
        r#"
#!/usr/bin/env node

// After the hashbang.

const x = 1;
import { a } from "a";
"#,
        HOIST,
        r#"
#!/usr/bin/env node

// After the hashbang.
import { a } from "a";

const x = 1;
"#,
    );
    assert_format(
        r#"
"use client";
// After the directive.
const x = 1;
import { a } from "a";
"#,
        HOIST,
        r#"
"use client";

// After the directive.
import { a } from "a";

const x = 1;
"#,
    );
    assert_format(
        r#"
#!/usr/bin/env node
import "zx/globals";
import { a } from "a";
"#,
        r#"{ "sortImports": { "hoist": true, "partitionBySideEffect": true } }"#,
        r#"
#!/usr/bin/env node

import "zx/globals";

import { a } from "a";
"#,
    );
}

#[test]
fn should_keep_comments_before_the_first_import_at_the_top() {
    assert_format(
        r#"
// Header.
import { b } from "b";
import { a } from "a";
"#,
        HOIST,
        r#"
// Header.
import { a } from "a";
import { b } from "b";
"#,
    );
    assert_format(
        r#"
/**
 * Header.
 */
import { b } from "b";
import { a } from "a";
"#,
        HOIST,
        r#"
/**
 * Header.
 */
import { a } from "a";
import { b } from "b";
"#,
    );
    // The comments of an import sorted to the front are separated by a blank line.
    assert_format(
        r#"
// Header.
import { b } from "b";
// About `a`.
import { a } from "a";
"#,
        HOIST,
        r#"
// Header.

// About `a`.
import { a } from "a";
import { b } from "b";
"#,
    );
}

#[test]
fn should_add_blank_line_after_trailing_comment() {
    assert_format(
        r#"
import { a } from "a"; // Trailing `a`.
// About `b`.
import { b } from "b";
"#,
        HOIST,
        r#"
import { a } from "a"; // Trailing `a`.

// About `b`.
import { b } from "b";
"#,
    );
    // The blank line is added when the next import did not start on the line after the comment.
    assert_format(
        r#"
import { a } from "a"; // Trailing `a`.
import { c } from "c";
const x = 1;
import { b } from "b";
"#,
        HOIST,
        r#"
import { a } from "a"; // Trailing `a`.

import { b } from "b";
import { c } from "c";

const x = 1;
"#,
    );
    // Below comments kept at the top, the first import counts as starting on the line of the first import.
    assert_format(
        r#"
// Header.
import { c } from "c";

import { b } from "b";
import { a } from "a"; // Trailing `a`.
"#,
        HOIST,
        r#"
// Header.
import { a } from "a"; // Trailing `a`.

import { b } from "b";
import { c } from "c";
"#,
    );
    assert_format(
        r#"
import { c } from "c";

import { b } from "b";
import { a } from "a"; // Trailing `a`.
"#,
        HOIST,
        r#"
import { a } from "a"; // Trailing `a`.
import { b } from "b";
import { c } from "c";
"#,
    );
    assert_format(
        r#"
import { b } from "b"; // Trailing `b`.
import { a } from "a";
"#,
        HOIST,
        r#"
import { a } from "a";
import { b } from "b"; // Trailing `b`.
"#,
    );
}

#[test]
fn should_move_comments_with_their_import() {
    assert_format(
        r#"
import { c } from "c";
// About `b`.

import { b } from "b";
import { a } from "a";
// About `d`.
import { d } from "d";
"#,
        HOIST,
        r#"
import { a } from "a";
// About `b`.

import { b } from "b";
import { c } from "c";
// About `d`.
import { d } from "d";
"#,
    );
    // A trailing comment after a multi-line import does not start on its line, so it stays with the code.
    assert_format(
        r#"
import { a } from "a";
const x = 1;
import {
  b,
} from "b"; // After `b`.
const y = 2;
"#,
        HOIST,
        r#"
import { a } from "a";
import { b } from "b";

const x = 1;
// After `b`.
const y = 2;
"#,
    );
}
