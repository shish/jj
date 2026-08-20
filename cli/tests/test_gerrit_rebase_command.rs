use crate::common::gerrit::GerritTestRepo;

#[test]
fn test_rebase_with_private_changes() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    let output = repo.run_jj(&["cr", "rebase"]);
    assert!(output.contains("Rebasing"));
}

#[test]
fn test_rebase_with_uploaded_changes() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let output = repo.run_jj(&["cr", "rebase"]);
    assert!(output.contains("Rebasing"));
}

#[test]
fn test_rebase_onto_updated_main() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let admin_clone = repo.fresh_clone();
    admin_clone.write_file("main_advance.txt", "Main advance content");
    admin_clone.run_jj(&["describe", "-m", "Advance main"]);
    admin_clone.run_jj(&["b", "a"]);
    admin_clone.run_jj(&["git", "push"]);

    repo.run_jj(&["git", "fetch"]);
    repo.run_jj(&["cr", "rebase"]);

    let head_description = repo.run_jj(&["log", "-r", "@", "--no-graph", "-T", "description"]);
    let parent_description = repo.run_jj(&["log", "-r", "@-", "--no-graph", "-T", "description"]);
    let grandparent_description =
        repo.run_jj(&["log", "-r", "@--", "--no-graph", "-T", "description"]);
    assert!(
        head_description.is_empty(),
        "expected @ to be an empty working-copy commit, got {head_description:?}"
    );
    assert!(parent_description.contains("Test commit 1"));
    assert!(grandparent_description.contains("Advance main"));
}
