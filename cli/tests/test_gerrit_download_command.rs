use crate::common::gerrit::GerritTestRepo;

#[test]
fn test_download() {
    let Some(repo) = GerritTestRepo::maybe_new() else {
        return;
    };

    repo.write_file("test_file.txt", "Test content");
    repo.run_jj(&["commit", "-m", "Test description"]);
    repo.run_jj(&["cr", "upload"]);

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    let change_num = changes[0]["id"]
        .as_str()
        .expect("change should have a string id")
        .trim_start_matches('c')
        .to_string();

    let clone2 = repo.fresh_clone();
    clone2.run_jj(&["cr", "download", &change_num]);
    assert!(clone2.file_exists("test_file.txt"));
    assert_eq!(clone2.file_contents("test_file.txt"), "Test content");
}
