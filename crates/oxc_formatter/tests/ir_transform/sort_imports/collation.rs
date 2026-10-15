use super::super::assert_format;

#[test]
fn should_sort_with_intl_collation() {
    let input = r##"
import { h } from "under-score";
import { u } from "under_score";
import { s } from "#lib/panels";
import { g } from "@grafana/sdk";
import { c } from "foo:bar";
import { d } from "foo/bar";
import { l10 } from "./log10";
import { l2 } from "./log2";
"##;
    assert_format(
        input,
        r#"{ "sortImports": { "collation": "intl", "newlinesBetween": false, "groups": [["external", "subpath", "sibling"]] } }"#,
        r##"
import { l2 } from "./log2";
import { l10 } from "./log10";
import { g } from "@grafana/sdk";
import { s } from "#lib/panels";
import { c } from "foo:bar";
import { d } from "foo/bar";
import { u } from "under_score";
import { h } from "under-score";
"##,
    );
    assert_format(
        input,
        r#"{ "sortImports": { "newlinesBetween": false, "groups": [["external", "subpath", "sibling"]] } }"#,
        r##"
import { s } from "#lib/panels";
import { l2 } from "./log2";
import { l10 } from "./log10";
import { g } from "@grafana/sdk";
import { d } from "foo/bar";
import { c } from "foo:bar";
import { h } from "under-score";
import { u } from "under_score";
"##,
    );
}

#[test]
fn should_compare_query_with_intl_collation() {
    // `-` sorts before `?`, so `./a-b` comes before `./a?url`.
    assert_format(
        r#"
import url from "./a?url";
import ab from "./a-b";
"#,
        r#"{ "sortImports": { "collation": "intl" } }"#,
        r#"
import ab from "./a-b";
import url from "./a?url";
"#,
    );
}

#[test]
fn should_break_case_ties_with_intl_collation_and_case_sensitivity() {
    let input = r#"
import b from "React";
import a from "react";
"#;
    // Equal at base sensitivity, so the input order is kept.
    assert_format(
        input,
        r#"{ "sortImports": { "collation": "intl" } }"#,
        r#"
import b from "React";
import a from "react";
"#,
    );
    assert_format(
        r#"
import a from "react";
import b from "React";
"#,
        r#"{ "sortImports": { "collation": "intl", "ignoreCase": false } }"#,
        r#"
import b from "React";
import a from "react";
"#,
    );
}
