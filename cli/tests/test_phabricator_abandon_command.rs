use crate::common::phabricator::PhabricatorTestRepo;

#[test]
fn test_abandon() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["commit", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload"]);

    let list = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert_eq!(revisions.len(), 1);

    let revision_id = revisions[0]["id"]
        .as_str()
        .expect("revision id should be a string");
    clone.run_jj(&["cr", "abandon", revision_id]);

    let list_after = clone.run_jj_json(&["cr", "list", "--json"]);
    let revisions_after = list_after
        .as_array()
        .expect("`jj cr list --json` should return an array");
    assert!(revisions_after.is_empty());
}
