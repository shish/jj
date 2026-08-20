use crate::common::gerrit::GerritTestRepo;

#[test]
fn test_log() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["gerrit", "upload", "--remote-branch", "master"]);
    repo.run_jj(&[
        "describe",
        "-r",
        "@-",
        "-m",
        "Test commit 1\n\nChange-Id: I1234567890abcdef1234567890abcdef12345678",
    ]);

    let log_output = repo.run_jj(&["cr", "log"]);
    assert!(log_output.contains("Test commit 1"));
}
