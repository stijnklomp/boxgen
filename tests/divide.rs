//! `boxgen divide` process behaviour: in-place edits, path resolution,
//! validation, and byte-for-byte failure safety.
//!
//! Contract: `docs/spec/docs/generator.md` §4 (division tree), §5 (CLI), and §7
//! (validation). A successful edit rewrites only the target node and exits 0;
//! an unresolved path or invalid result exits 2 and leaves the file untouched;
//! missing flags are a usage error (exit 1) and also leave the file untouched.

use std::fs;
use std::path::{Path, PathBuf};

mod common;

use assert_cmd::assert::Assert;
use predicates::prelude::*;

use boxgen::spec;
use boxgen::tree::{Axis, Node};

fn fixture_contents(name: &str) -> String {
    fs::read_to_string(common::fixture(name)).expect("fixture must be readable")
}

fn temp_spec(contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temporary directory must be creatable");
    let path = dir.path().join("spec.yaml");
    fs::write(&path, contents).expect("temporary spec must be writable");
    (dir, path)
}

fn fixture_copy(name: &str) -> (tempfile::TempDir, PathBuf) {
    temp_spec(&fixture_contents(name))
}

fn divide(path: &Path, args: &[&str]) -> Assert {
    let mut command = common::boxgen();
    command.arg("divide").arg(path).args(args).assert()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("spec must be readable")
}

fn validate(path: &Path) -> Assert {
    let mut command = common::boxgen();
    command.arg("validate").arg(path).assert()
}

/// Runs a failing divide and asserts the spec file is byte-for-byte unchanged.
fn assert_failure_keeps_file(contents: &str, args: &[&str], code: i32, needle: &str) {
    let (_dir, path) = temp_spec(contents);
    let before = fs::read(&path).expect("spec must be readable");

    divide(&path, args)
        .failure()
        .code(code)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(needle));

    let after = fs::read(&path).expect("spec must be readable");
    assert_eq!(
        before, after,
        "a failed divide must not change the file ({needle})"
    );
}

#[test]
fn editing_a_split_changes_only_axis_and_position() {
    let (_dir, path) = fixture_copy("4x4-6u.yaml");
    let before = read(&path);

    divide(&path, &["--path", "root.near", "--axis", "x", "--at", "40"])
        .success()
        .code(0)
        .stderr(predicate::str::is_empty());

    let after = read(&path);
    let expected = before.replacen(
        "    axis: y\n    at_mm: 60\n",
        "    axis: x\n    at_mm: 40\n",
        1,
    );
    assert_eq!(after, expected);
    validate(&path).success().code(0);
}

#[test]
fn one_flag_keeps_the_other_split_field() {
    let (_dir, path) = fixture_copy("4x4-6u.yaml");

    divide(&path, &["--path", "root.near", "--at", "50"])
        .success()
        .code(0);
    assert!(read(&path).contains("    axis: y\n    at_mm: 50\n"));

    divide(&path, &["--path", "root.near", "--axis", "x"])
        .success()
        .code(0);
    assert!(read(&path).contains("    axis: x\n    at_mm: 50\n"));
    validate(&path).success().code(0);
}

#[test]
fn a_root_leaf_becomes_a_split_and_keeps_its_name_on_the_near_child() {
    let (_dir, path) = fixture_copy("minimal.yaml");

    divide(&path, &["--axis", "x", "--at", "18"])
        .success()
        .code(0)
        .stderr(predicate::str::is_empty());

    let spec = spec::load(&path).expect("edited spec must load");
    match spec.divisions {
        Node::Split {
            axis,
            at_mm,
            near,
            far,
        } => {
            assert_eq!(axis, Axis::X);
            assert_eq!(at_mm, 18);
            assert_eq!(*near, Node::leaf(Some("Bits")));
            assert_eq!(*far, Node::leaf(None));
        }
        other => panic!("expected a split, got {other:?}"),
    }

    validate(&path).success().code(0);
}

#[test]
fn a_nested_leaf_becomes_a_split() {
    let (_dir, path) = fixture_copy("4x4-6u.yaml");

    divide(
        &path,
        &["--path", "root.far.near", "--axis", "x", "--at", "40"],
    )
    .success()
    .code(0);

    let spec = spec::load(&path).expect("edited spec must load");
    let Node::Split { far, .. } = &spec.divisions else {
        panic!("root must stay a split");
    };
    let Node::Split { near, .. } = far.as_ref() else {
        panic!("root.far must stay a split");
    };
    let Node::Split {
        axis,
        at_mm,
        near,
        far,
    } = near.as_ref()
    else {
        panic!("root.far.near must become a split");
    };

    assert_eq!(*axis, Axis::X);
    assert_eq!(*at_mm, 40);
    assert_eq!(**near, Node::leaf(Some("Power")));
    assert_eq!(**far, Node::leaf(None));

    validate(&path).success().code(0);
}

#[test]
fn comments_and_unrelated_formatting_survive_a_split_edit() {
    let (_dir, path) = fixture_copy("divide-comments.yaml");
    let before = read(&path);

    divide(&path, &["--path", "root.near", "--axis", "x", "--at", "40"])
        .success()
        .code(0);

    let after = read(&path);
    for line in before.lines().filter(|line| line.contains('#')) {
        let comment = line.split('#').nth(1).expect("comment line");
        assert!(
            after.contains(comment.trim_end()),
            "comment lost: {line}\n--- edited ---\n{after}"
        );
    }

    let expected = before
        .replacen(
            "    axis: y          # near axis comment",
            "    axis: x          # near axis comment",
            1,
        )
        .replacen(
            "    at_mm: 60        # near position comment",
            "    at_mm: 40        # near position comment",
            1,
        );
    assert_eq!(after, expected);
    validate(&path).success().code(0);
}

#[test]
fn comments_survive_a_leaf_replacement() {
    let (_dir, path) = fixture_copy("divide-comments-leaf.yaml");
    let before = read(&path);

    divide(&path, &["--axis", "x", "--at", "18"])
        .success()
        .code(0);

    let after = read(&path);
    let header = before
        .split("divisions:")
        .next()
        .expect("fixture has a divisions block");
    assert!(
        after.starts_with(header),
        "unrelated formatting changed:\n{after}"
    );
    for line in before.lines().filter(|line| line.contains('#')) {
        let comment = line.split('#').nth(1).expect("comment line");
        assert!(
            after.contains(comment.trim_end()),
            "comment lost: {line}\n--- edited ---\n{after}"
        );
    }
    validate(&path).success().code(0);
}

#[test]
fn a_leaf_below_the_minimum_is_rejected_and_the_file_is_unchanged() {
    assert_failure_keeps_file(
        &fixture_contents("minimal.yaml"),
        &["--axis", "y", "--at", "8"],
        2,
        "at_mm 8 is below the 12 mm minimum",
    );
}

#[test]
fn validation_errors_name_the_rule_and_field_path() {
    let (_dir, path) = temp_spec(&fixture_contents("minimal.yaml"));

    divide(&path, &["--axis", "y", "--at", "8"])
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "at_mm 8 is below the 12 mm minimum",
        ))
        .stderr(predicate::str::contains(
            "divisions.near: leaf 8.0 mm deep is below the 12 mm minimum",
        ))
        .stderr(predicate::str::contains("path `root`"));
}

#[test]
fn a_far_child_below_the_minimum_is_rejected_and_the_file_is_unchanged() {
    assert_failure_keeps_file(
        &fixture_contents("minimal.yaml"),
        &["--axis", "x", "--at", "30"],
        2,
        "leaf 5.5 mm wide",
    );
}

#[test]
fn a_coincident_divider_is_rejected_and_the_file_is_unchanged() {
    assert_failure_keeps_file(
        &fixture_contents("divide-coincident.yaml"),
        &["--path", "root.near", "--at", "80"],
        2,
        "coincides with an ancestor divider",
    );
}

#[test]
fn a_tree_deeper_than_thirty_two_is_rejected_and_the_file_is_unchanged() {
    let depth = 32;
    let text = format!(
        "system: SYS-1.0\nid: 4x4-6u-deep-v1\nfootprint: [4, 4]\nheight_u: 6\nlid: false\ndivisions:\n{}",
        deep_divisions(depth)
    );
    let path = format!("root{}", ".near".repeat(depth));

    assert_failure_keeps_file(
        &text,
        &["--path", &path, "--axis", "x", "--at", "12"],
        2,
        "tree depth 33 exceeds the 32 limit",
    );
}

/// Builds a chain of `depth` x splits with `near` nested and a far leaf at
/// every level, ending in a named leaf.
fn deep_divisions(depth: usize) -> String {
    fn build(depth: usize, indent: usize) -> String {
        let pad = " ".repeat(indent);
        let mut out = format!("{pad}axis: x\n{pad}at_mm: 12\n{pad}near:\n");
        if depth == 1 {
            out.push_str(&format!("{}compartment: Deep\n", " ".repeat(indent + 2)));
        } else {
            out.push_str(&build(depth - 1, indent + 2));
        }
        out.push_str(&format!("{pad}far: {{}}\n"));
        out
    }

    build(depth, 2)
}

#[test]
fn unresolved_paths_are_rejected_and_the_file_is_unchanged() {
    let contents = fixture_contents("4x4-6u.yaml");

    assert_failure_keeps_file(
        &contents,
        &["--path", "root.near.far.near", "--axis", "x", "--at", "40"],
        2,
        "is a leaf",
    );
    assert_failure_keeps_file(
        &contents,
        &["--path", "root.middle", "--axis", "x", "--at", "40"],
        2,
        "unknown segment `middle`",
    );
    assert_failure_keeps_file(
        &contents,
        &["--path", "root.", "--axis", "x", "--at", "40"],
        2,
        "empty path segment",
    );
}

#[test]
fn missing_flags_are_a_usage_error_and_the_file_is_unchanged() {
    assert_failure_keeps_file(
        &fixture_contents("4x4-6u.yaml"),
        &["--path", "root.near"],
        1,
        "provide --axis and/or --at",
    );
    assert_failure_keeps_file(
        &fixture_contents("minimal.yaml"),
        &["--axis", "x"],
        1,
        "provide both --axis and --at",
    );
    assert_failure_keeps_file(
        &fixture_contents("minimal.yaml"),
        &["--at", "20"],
        1,
        "provide both --axis and --at",
    );
}
