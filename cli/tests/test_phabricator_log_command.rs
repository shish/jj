use crate::common::phabricator::PhabricatorTestRepo;

#[test]
fn test_log() {
    let Some(clone) = PhabricatorTestRepo::maybe_new() else {
        return;
    };
    clone.write_file("test_file.txt", "Test content");
    clone.run_jj(&["commit", "-m", "Test commit 1"]);
    clone.run_jj(&["cr", "upload"]);
    let output = clone.run_jj(&["cr", "log"]);
    assert!(output.contains("Test commit 1"));
}
