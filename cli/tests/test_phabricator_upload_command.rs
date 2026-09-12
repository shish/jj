use crate::common::phabricator::PhabricatorTestRepo;

use jj_cli::commands::phabricator::upload::change_to_revision;

#[test]
fn test_change_to_revision() {
    assert_eq!(
        change_to_revision("subject\n\nDifferential Revision: https://mycorp.com/D1234"),
        Some(1234)
    );
    assert_eq!(change_to_revision("subject"), None);
}

#[test]
fn test_push_one_head() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["commit", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload", "-m", "Test push 1"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["title"].as_str(), Some("Test commit 1"));
    assert_eq!(revisions[0]["state_name"].as_str(), Some("Needs Review"));
}

#[test]
fn test_push_one_head_draft() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["commit", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload", "--draft"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["title"].as_str(), Some("Test commit 1"));
    assert_eq!(revisions[0]["state_name"].as_str(), Some("Draft"));
}

#[test]
fn test_push_one_cwd() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["describe", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload", "-m", "Test push 1"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["title"].as_str(), Some("Test commit 1"));
}

#[test]
fn test_push_one_then_update() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["describe", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload", "-m", "Test push 1"]);

    clone.write_file("test_file.txt", "Amended content");
    clone.run_jj(&["squash"]);
    clone.run_jj(&["cr", "upload", "-m", "Test amend"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["title"].as_str(), Some("Test commit 1"));
}

#[test]
fn test_push_one_then_two() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["commit", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload", "-m", "Test push 1"]);

    clone.write_file("test_file2.txt", "Test content 2");
    clone.run_jj(&["commit", "-m", "Test commit 2"]);
    clone.run_jj(&["cr", "upload", "-m", "Test push 2"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 2, "{revisions:#?}");
    assert_eq!(revisions[0]["title"].as_str(), Some("Test commit 2"));
    assert_eq!(revisions[1]["title"].as_str(), Some("Test commit 1"));
}

#[test]
fn test_push_two_at_once() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["commit", "-m", "Test commit 1"]);

    clone.write_file("test_file2.txt", "Test content 2");
    clone.run_jj(&["commit", "-m", "Test commit 2"]);

    clone.run_jj(&["cr", "upload", "-m", "Test push 1+2"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0]["title"].as_str(), Some("Test commit 2"));
    assert_eq!(revisions[1]["title"].as_str(), Some("Test commit 1"));
}
