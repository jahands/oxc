//! `tests/fixtures/js_compat/<name>.sorted.json` is the output of the npm package
//! `sort-package-json` 3.5.0 for `<name>.json`: `JSON.stringify(sortPackageJson(input), null, 2)`.

use std::fs;

use sort_package_json::{SortOptions, SortOrder, sort_package_json_with_options};

fn sort(input: &str) -> String {
    let options = SortOptions::new().with_order(SortOrder::SortPackageJson);
    sort_package_json_with_options(input, &options).expect("valid JSON")
}

#[test]
fn matches_sort_package_json_3_5_0() {
    let mut count = 0;
    for entry in fs::read_dir("tests/fixtures/js_compat").unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if name.ends_with(".sorted.json") {
            continue;
        }
        let input = fs::read_to_string(&path).unwrap();
        let expected = fs::read_to_string(path.with_extension("sorted.json")).unwrap();
        let actual = sort(&input);
        assert_eq!(actual, expected, "{name}");
        assert_eq!(sort(&actual), expected, "{name} is not idempotent");
        count += 1;
    }
    assert!(count > 0);
}

#[test]
fn ignores_sort_scripts() {
    let input = r#"{"scripts":{"b":"","prea":"","a":""}}"#;
    let options = SortOptions::new()
        .with_pretty(false)
        .with_sort_scripts(false)
        .with_order(SortOrder::SortPackageJson);
    assert_eq!(
        sort_package_json_with_options(input, &options).unwrap(),
        r#"{"scripts":{"prea":"","a":"","b":""}}"#
    );
}
