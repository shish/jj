use crate::common::phabricator::PhabricatorTestRepo;

#[test]
fn test_download() {
    let Some(clone1) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone1.write_file("test_file.txt", "Test content");
    clone1.run_jj(&["commit", "-m", "Test description"]);
    clone1.run_jj(&["cr", "upload"]);

    let list = clone1.run_jj_json(&["cr", "list", "--json"]);
    let revisions = list
        .as_array()
        .expect("`jj cr list --json` should return an array");
    let revision_id = revisions[0]["id"]
        .as_str()
        .expect("revision id should be a string");

    let clone2 = clone1.fresh_clone();
    clone2.run_jj(&["cr", "download", revision_id]);

    assert!(clone2.file_exists("test_file.txt"));
    assert_eq!(clone2.read_file("test_file.txt"), "Test content");
}
