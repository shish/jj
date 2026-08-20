use crate::common::github::cloned_repo;
use crate::common::github::repo_path;
use crate::common::github::run_jj;
use crate::common::github::run_jj_json;

#[test]
fn test_push_one_head() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test commit 1"]);
    run_jj(&env, ["cr", "upload"]);

    let list = run_jj_json(&env, ["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["title"]["text"].as_str(), Some("Test commit 1"));
}

#[test]
fn test_push_one_cwd() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test commit 1"]);
    run_jj(&env, ["cr", "upload"]);

    let list = run_jj_json(&env, ["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["title"]["text"].as_str(), Some("Test commit 1"));
}

#[test]
fn test_push_one_then_two() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test commit 1"]);
    run_jj(&env, ["cr", "upload"]);

    std::fs::write(repo_path(&env).join("test_file2.txt"), "Test content 2")
        .expect("failed writing second test file");
    run_jj(&env, ["commit", "-m", "Test commit 2"]);
    run_jj(&env, ["cr", "upload"]);

    let list = run_jj_json(&env, ["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["title"]["text"].as_str(), Some("Test commit 1"));
}

#[test]
fn test_push_two_at_once() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test commit 1"]);

    std::fs::write(repo_path(&env).join("test_file2.txt"), "Test content 2")
        .expect("failed writing second test file");
    run_jj(&env, ["commit", "-m", "Test commit 2"]);

    run_jj(&env, ["cr", "upload"]);

    let list = run_jj_json(&env, ["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["title"]["text"].as_str(), Some("Test commit 2"));
}
