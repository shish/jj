use crate::common::github::cloned_repo;
use crate::common::github::repo_path;
use crate::common::github::run_jj;

#[test]
fn test_log() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test commit 1"]);
    run_jj(&env, ["cr", "upload"]);

    let log_output = run_jj(&env, ["cr", "log"]);
    assert!(log_output.contains("Test commit 1"));
}
