use crate::common::gerrit::GerritTestRepo;

#[test]
fn test_abandon() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    assert_eq!(changes.len(), 1);
    let cr_id = changes[0]["id"]
        .as_str()
        .expect("change should have a string id")
        .to_string();

    repo.run_jj(&["cr", "abandon", &cr_id]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    assert!(changes.is_empty());
}
