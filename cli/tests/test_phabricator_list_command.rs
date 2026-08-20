use crate::common::phabricator::PhabricatorTestRepo;

#[test]
fn test_list_empty() {
    let Some(repo) = PhabricatorTestRepo::maybe_new() else {
        return;
    };

    let text = repo.run_jj(&["cr", "list"]);
    assert_eq!(
        text,
        "┌────┬───────┬───────┬────────┬──────────┐\n│ ID ┆ Title ┆ State ┆ Checks ┆ Comments │\n╞════╪═══════╪═══════╪════════╪══════════╡\n└────┴───────┴───────┴────────┴──────────┘"
    );

    let list = repo.run_jj_json(&["cr", "list", "--json"]);
    let changes = list.as_array().expect("list --json should return an array");
    assert!(changes.is_empty());
}
