use super::super::assert_format;

#[test]
fn should_sort_specifiers() {
    assert_format(
        r#"
import Def, { Zed, type Gamma, alpha, delta as aaa, Beta, x10, x9, $x, _y } from "spec";
import * as ns from "ns";
import { only } from "only";
"#,
        r#"{ "sortImports": { "sortSpecifiers": true, "collation": "intl" } }"#,
        r#"
import * as ns from "ns";
import { only } from "only";
import Def, { _y, $x, delta as aaa, alpha, Beta, x9, x10, Zed, type Gamma } from "spec";
"#,
    );
    assert_format(
        r#"
import { Zed, type Gamma, alpha, delta as aaa, Beta, x10, x9, $x, _y } from "spec";
"#,
        r#"{ "sortImports": { "sortSpecifiers": true } }"#,
        r#"
import { $x, _y, delta as aaa, alpha, Beta, x9, x10, Zed, type Gamma } from "spec";
"#,
    );
}

#[test]
fn should_sort_specifiers_in_type_imports() {
    assert_format(
        r#"
import type { Q, P } from "tq";
"#,
        r#"{ "sortImports": { "sortSpecifiers": true } }"#,
        r#"
import type { P, Q } from "tq";
"#,
    );
}

#[test]
fn should_sort_specifiers_in_descending_order() {
    assert_format(
        r#"
import { a, type T, c, b } from "x";
"#,
        r#"{ "sortImports": { "sortSpecifiers": true, "order": "desc" } }"#,
        r#"
import { c, b, a, type T } from "x";
"#,
    );
}

#[test]
fn should_break_sorted_specifiers_by_their_new_order() {
    assert_format(
        r#"
import {
  cccccccccccccccccccccccccccccc,
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
} from "x";
"#,
        r#"{ "sortImports": { "sortSpecifiers": true } }"#,
        r#"
import {
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
  cccccccccccccccccccccccccccccc,
} from "x";
"#,
    );
}

#[test]
fn should_not_sort_specifiers_with_comments() {
    assert_format(
        r#"
import {
  // comment
  b,
  a,
} from "x";
import { d, /* c */ c } from "y";
"#,
        r#"{ "sortImports": { "sortSpecifiers": true } }"#,
        r#"
import {
  // comment
  b,
  a,
} from "x";
import { d, /* c */ c } from "y";
"#,
    );
}

#[test]
fn should_not_sort_specifiers_by_default() {
    assert_format(
        r#"
import { b, a } from "x";
"#,
        r#"{ "sortImports": {} }"#,
        r#"
import { b, a } from "x";
"#,
    );
}
