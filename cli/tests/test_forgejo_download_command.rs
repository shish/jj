use crate::common::forgejo::ForgejoTestRepo;

#[test]
fn test_download() {
    let Some(repo) = ForgejoTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test description"]);
    repo.run_jj(&["cr", "upload"]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    let pr_id = prs[0]["id"].as_str().expect("PR should have id").to_owned();

    let clone = repo.fresh_clone();
    clone.run_jj(&["cr", "download", &pr_id]);

    assert!(clone.file_exists("test_file.txt"));
    assert_eq!(clone.read_file("test_file.txt"), "Test content");
}
