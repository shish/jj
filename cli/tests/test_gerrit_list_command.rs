use crate::common::gerrit::GerritTestRepo;

#[test]
fn test_list_empty() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    let text = repo.run_jj(&["cr", "list"]);
    assert_eq!(
        text,
        "┌────┬───────┬───────┬────────┬──────────┐\n│ ID ┆ Title ┆ State ┆ Checks ┆ Comments │\n╞════╪═══════╪═══════╪════════╪══════════╡\n└────┴───────┴───────┴────────┴──────────┘\n"
    );

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    assert!(changes.is_empty());
}

#[test]
fn test_list_one() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let text = repo.run_jj(&["cr", "list"]);
    assert!(!text.is_empty());

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["title"].as_str(), Some("Test commit 1"));
    assert!(
        changes[0]["id"]
            .as_str()
            .expect("change should have a string id")
            .starts_with('c')
    );
}

#[test]
fn test_list_multiple() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);

    repo.write_file("test_file2.txt", "Test content 2");
    repo.run_jj(&["commit", "-m", "Test commit 2"]);

    repo.run_jj(&["cr", "upload"]);

    let text = repo.run_jj(&["cr", "list"]);
    assert!(!text.is_empty());

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    assert_eq!(changes.len(), 2);

    let titles = changes
        .iter()
        .map(|item| {
            item["title"]
                .as_str()
                .expect("change title should be a string")
        })
        .collect::<std::collections::HashSet<_>>();
    assert!(titles.contains("Test commit 1"));
    assert!(titles.contains("Test commit 2"));
}
