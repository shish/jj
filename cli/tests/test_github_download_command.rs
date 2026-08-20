use crate::common::TestEnvironment;
use crate::common::github::cloned_repo;
use crate::common::github::maybe_external_github_repo_url;
use crate::common::github::repo_path;
use crate::common::github::run_jj;
use crate::common::github::run_jj_json;

#[test]
fn test_download() {
    let Some(env) = cloned_repo() else {
        return;
    };

    std::fs::write(repo_path(&env).join("test_file.txt"), "Test content")
        .expect("failed writing test file");
    run_jj(&env, ["commit", "-m", "Test description"]);
    run_jj(&env, ["cr", "upload"]);

    let list = run_jj_json(&env, ["cr", "list", "--json"]);
    let prs = list.as_array().expect("expected JSON array from cr list");
    let pr_id = prs[0]["id"].as_str().expect("PR should have id").to_owned();

    let Some(repo_url) = maybe_external_github_repo_url() else {
        return;
    };
    let env2 = TestEnvironment::default();
    env2.work_dir("repo")
        .run_jj(["git", "clone", repo_url.as_str(), "."])
        .success();
    run_jj(&env2, ["cr", "download", pr_id.as_str()]);

    let downloaded_file = repo_path(&env2).join("test_file.txt");
    assert!(downloaded_file.exists());
    assert_eq!(
        std::fs::read_to_string(downloaded_file).expect("failed reading downloaded file"),
        "Test content"
    );
}
