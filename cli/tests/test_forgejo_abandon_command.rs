use serde_json::Value;

use crate::common::forgejo::ForgejoTestRepo;

#[test]
fn test_abandon() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let initial = repo.run_jj_json(&["cr", "list", "--json"]);
    let prs = initial
        .as_array()
        .expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    let pr_id = prs[0]["id"].as_str().expect("PR should have id").to_owned();

    repo.run_jj(&["cr", "abandon", &pr_id, "-m", "Not needed after all"]);

    let after = repo.run_jj_json(&["cr", "list", "--json"]);
    assert_eq!(after, Value::Array(vec![]));
}
