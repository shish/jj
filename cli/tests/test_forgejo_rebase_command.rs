use crate::common::forgejo::ForgejoTestRepo;

#[test]
fn test_rebase_onto_pr_base() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("first.txt", "First");
    repo.run_jj(&["commit", "-m", "First commit"]);
    repo.write_file("second.txt", "Second");
    repo.run_jj(&["commit", "-m", "Second commit"]);
    repo.run_jj(&["cr", "upload"]);

    // Both commits already sit on their pull requests' base branches, so
    // rebasing them there again should keep the stack intact.
    repo.run_jj(&["cr", "rebase", "--all-prs"]);

    let log = repo.run_jj(&["log", "--no-graph", "-T", r#"description ++ "\n""#]);
    assert!(log.contains("First commit"));
    assert!(log.contains("Second commit"));
}
