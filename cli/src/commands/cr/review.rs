use std::collections::BTreeMap;
use std::collections::HashMap;

use comfy_table::Cell;
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL_CONDENSED;
pub use jj_lib::backend::CommitId;
pub use jj_lib::extensions_map::ExtensionsMap;
pub use jj_lib::review::CheckState;
pub use jj_lib::review::CodeReview;
pub use jj_lib::review::CodeReviewLookup;

use crate::commit_templater::CommitTemplateBuildFnTable;
use crate::commit_templater::CommitTemplateLanguageExtension;

/// Makes reviews fetched for one `cr log` invocation available to commit
/// templates without affecting ordinary `jj log` invocations.
pub(crate) struct CodeReviewTemplateLanguageExtension {
    lookup: CodeReviewLookup,
}

impl CodeReviewTemplateLanguageExtension {
    pub(crate) fn new(
        identifiers_by_commit: BTreeMap<CommitId, String>,
        reviews_by_identifier: BTreeMap<String, CodeReview>,
    ) -> Self {
        Self {
            lookup: CodeReviewLookup::new(identifiers_by_commit, reviews_by_identifier),
        }
    }
}

impl CommitTemplateLanguageExtension for CodeReviewTemplateLanguageExtension {
    fn build_fn_table<'repo>(&self) -> CommitTemplateBuildFnTable<'repo> {
        CommitTemplateBuildFnTable::empty()
    }

    fn build_cache_extensions(&self, extensions: &mut ExtensionsMap) {
        extensions.insert(self.lookup.clone());
    }
}

pub fn display_table(reviews: Vec<CodeReview>) -> std::io::Result<()> {
    let mut table = Table::new();
    table.set_content_arrangement(comfy_table::ContentArrangement::Dynamic);
    table.load_style(UTF8_FULL_CONDENSED);
    table.set_header(vec!["ID", "Title", "State", "Checks", "Comments"]);
    table.set_width(79);
    for review in &reviews {
        let checks = if review.checks.len() < 8 {
            review
                .checks
                .iter()
                .map(|check| check.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            // map<CheckState, count>
            let mut map: HashMap<CheckState, i64> = HashMap::new();
            for check in &review.checks {
                *map.entry(check.state).or_default() += 1;
            }
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by_key(|(state, _)| *state);
            format!(
                "{}",
                sorted
                    .iter()
                    .map(|(state, count)| format!("{}{}", state.symbol(), count))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        table.add_row(vec![
            Cell::new(if let Some(version) = review.version {
                format!("{}/{}", review.id, version)
            } else {
                review.id.clone()
            }),
            Cell::new(&review.title),
            Cell::new(&review.state_name),
            Cell::new(checks),
            Cell::new(review.unresolved_comments.to_string()),
        ]);
    }
    println!("{table}");
    Ok(())
}
