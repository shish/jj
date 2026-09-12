use crate::common::forgejo::ForgejoTestRepo;

#[test]
fn test_log() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let log_output = repo.run_jj(&["cr", "log"]);
    assert!(log_output.contains("Test commit 1"));
}
