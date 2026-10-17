//! `boxgen index` process contract.
//!
//! The format is pinned against the spec repository's committed `boxes/INDEX.md`
//! when that checkout is available beside this one: copy its `boxes/` tree into
//! a temp directory, run `index`, and require byte-for-byte equality. The
//! checkout is local-only, so the test skips cleanly when it is absent.

use std::fs;
use std::path::{Path, PathBuf};

mod common;

use predicates::prelude::*;
use tempfile::tempdir;

/// Locates the sibling SYS-1.0 spec checkout from a main clone or a worktree.
fn spec_repository() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let relative = "research/3d-printed-stackable-portable-storage";

    [
        manifest.join("..").join(relative),
        manifest.join("..").join("..").join(relative),
        manifest
            .join("..")
            .join("3d-printed-stackable-portable-storage"),
    ]
    .into_iter()
    .find(|candidate| {
        candidate.join("boxes/INDEX.md").is_file() && candidate.join("docs/generator.md").is_file()
    })
}

fn copy_directory(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create destination directory");
    for entry in fs::read_dir(from).expect("read source directory") {
        let entry = entry.expect("read source entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("source file type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// The empty index is the committed spec-repository file byte for byte.
///
/// `tests/fixtures/committed-INDEX.md` is a copy of the spec repository's
/// `boxes/INDEX.md`, so this comparison also runs in the gate container, which
/// does not mount the sibling checkout.
#[test]
fn empty_index_matches_the_committed_file_byte_for_byte() {
    let temp = tempdir().expect("tempdir");
    fs::create_dir(temp.path().join("boxes")).expect("boxes directory");

    common::boxgen()
        .arg("index")
        .current_dir(temp.path())
        .assert()
        .success();

    let generated =
        fs::read_to_string(temp.path().join("boxes/INDEX.md")).expect("generated index");
    assert_eq!(
        generated,
        include_str!("fixtures/committed-INDEX.md"),
        "the empty index must match the committed spec-repository format"
    );
}

/// The generated index reproduces the format already committed in the spec
/// repository, byte for byte, through the default `--boxes` path.
#[test]
fn index_reproduces_the_committed_spec_repository_index() {
    let Some(repository) = spec_repository() else {
        eprintln!(
            "skipping committed-index comparison: the SYS-1.0 spec checkout is not beside this repository"
        );
        return;
    };

    let temp = tempdir().expect("tempdir");
    copy_directory(&repository.join("boxes"), &temp.path().join("boxes"));

    common::boxgen()
        .arg("index")
        .current_dir(temp.path())
        .assert()
        .success();

    let generated =
        fs::read_to_string(temp.path().join("boxes/INDEX.md")).expect("generated index");
    let committed = fs::read_to_string(repository.join("boxes/INDEX.md")).expect("committed index");
    assert_eq!(
        generated, committed,
        "boxgen index must reproduce the committed INDEX.md byte for byte"
    );
}

/// A malformed or invalid spec in the registry is exit 2 and names the file
/// and the rule; the index is not written.
#[test]
fn an_invalid_spec_in_the_registry_exits_two_without_writing_the_index() {
    let temp = tempdir().expect("tempdir");
    let folder = temp.path().join("boxes/broken");
    fs::create_dir_all(&folder).expect("box folder");
    fs::write(
        folder.join("spec.yaml"),
        "system: SYS-1.0\nid: 5x5-6U-broken-v1\nfootprint: [5, 5]\nheight_u: 6\nlid: false\ndivisions: { compartment: A }\n",
    )
    .expect("write spec");

    common::boxgen()
        .args(["index", "--boxes"])
        .arg(temp.path().join("boxes"))
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains("broken/spec.yaml")
                .and(predicate::str::contains("outside 1..=4")),
        );

    assert!(!temp.path().join("boxes/INDEX.md").exists());
}
