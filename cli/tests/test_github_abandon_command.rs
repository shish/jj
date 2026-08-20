use serde_json::Value;

use crate::common::github::cloned_repo;
use crate::common::github::repo_path;
use crate::common::github::run_jj;
use crate::common::github::run_jj_json;

#[test]
fn test_abandon() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test commit 1"]);
    run_jj(&env, ["cr", "upload"]);

    let initial = run_jj_json(&env, ["cr", "list", "--json"]);
    let prs = initial
        .as_array()
        .expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    let pr_id = prs[0]["id"].as_str().expect("PR should have id").to_owned();

    run_jj(&env, ["cr", "abandon", pr_id.as_str()]);

    let after = run_jj_json(&env, ["cr", "list", "--json"]);
    assert_eq!(after, Value::Array(vec![]));
}
