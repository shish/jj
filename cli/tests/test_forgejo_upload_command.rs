use crate::common::forgejo::ForgejoTestRepo;

#[test]
fn test_push_one() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["title"].as_str(), Some("Test commit 1"));
    assert_eq!(prs[0]["state"].as_str(), Some("NeedsReview"));
}

#[test]
fn test_push_one_draft() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload", "--draft"]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["title"].as_str(), Some("WIP: Test commit 1"));
    assert_eq!(prs[0]["state"].as_str(), Some("Draft"));
}

#[test]
fn test_push_one_then_two() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.run_jj(&["cr", "upload"]);

    repo.write_file("test_file2.txt", "Test content 2");
    repo.run_jj(&["commit", "-m", "Test commit 2"]);
    repo.run_jj(&["cr", "upload"]);

    // The first upload created one PR, the second one updated it and
    // created a second PR stacked on top of it.
    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 2);
    let titles: Vec<_> = prs.iter().map(|pr| pr["title"].as_str()).collect();
    assert!(titles.contains(&Some("Test commit 1")));
    assert!(titles.contains(&Some("Test commit 2")));
}

#[test]
fn test_push_two_at_once() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test commit 1"]);
    repo.write_file("test_file2.txt", "Test content 2");
    repo.run_jj(&["commit", "-m", "Test commit 2"]);
    repo.run_jj(&["cr", "upload"]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    assert_eq!(prs.len(), 2);
    let titles: Vec<_> = prs.iter().map(|pr| pr["title"].as_str()).collect();
    assert!(titles.contains(&Some("Test commit 1")));
    assert!(titles.contains(&Some("Test commit 2")));
}
