use std::collections::BTreeMap;

use comfy_table::Cell;
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL_CONDENSED;
use jj_lib::backend::CommitId;
use jj_lib::extensions_map::ExtensionsMap;
use url::Url;

use crate::commit_templater::CommitTemplateBuildFnTable;
use crate::commit_templater::CommitTemplateLanguageExtension;

///! Common code review types and utilities.

/// A minimal, forge-agnostic representation of a code review (PR/CR/Diff),
/// as returned by the `<forge> list` and `<forge> log` commands.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CodeReview {
    pub id: String,
    pub title: String,
    pub url: Url,
    pub state_name: String,
    pub state: CodeReviewState,
    pub checks: Vec<Check>,
    pub unresolved_comments: i64,
}

/// The category of a CR's overall review state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum CodeReviewState {
    Accepted,
    Rejected,
    NeedsReview,
    Draft,
    Blocked,
    Private,
    Closed,
    Other,
}

impl CodeReview {
    pub fn label(self) -> &'static str {
        match self.state {
            CodeReviewState::Accepted => "accepted",
            CodeReviewState::Rejected => "rejected",
            CodeReviewState::NeedsReview => "needs-review",
            CodeReviewState::Draft => "draft",
            CodeReviewState::Blocked => "blocked",
            CodeReviewState::Private => "private",
            CodeReviewState::Closed => "closed",
            CodeReviewState::Other => "other",
        }
    }
}

/// The outcome of a single CI check or submit requirement.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Check {
    pub name: String,
    pub url: Option<Url>,
    pub state: CheckState,
}

impl Check {
    pub fn label(&self) -> &'static str {
        self.state.label()
    }

    pub fn icon(&self) -> &'static str {
        self.state.symbol()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum CheckState {
    Pass,
    Fail,
    InProgress,
    Other,
    Unknown,
}

impl CheckState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "passed",
            Self::Fail => "failed",
            Self::InProgress => "in-progress",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Pass => "✓",
            Self::Fail => "✗",
            Self::InProgress => "…",
            Self::Other => "•",
            Self::Unknown => "?",
        }
    }
}

/// Maps commits to forge identifiers and identifiers to their fetched reviews.
#[derive(Clone)]
pub(crate) struct CodeReviewLookup {
    identifiers_by_commit: BTreeMap<CommitId, String>,
    reviews_by_identifier: BTreeMap<String, CodeReview>,
}

impl CodeReviewLookup {
    pub(crate) fn new(
        identifiers_by_commit: BTreeMap<CommitId, String>,
        reviews_by_identifier: BTreeMap<String, CodeReview>,
    ) -> Self {
        Self {
            identifiers_by_commit,
            reviews_by_identifier,
        }
    }

    pub(crate) fn review(&self, commit_id: &CommitId) -> Option<CodeReview> {
        self.identifiers_by_commit
            .get(commit_id)
            .and_then(|identifier| self.reviews_by_identifier.get(identifier))
            .cloned()
    }
}

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
    table.load_style(UTF8_FULL_CONDENSED);
    table.set_header(vec!["ID", "Title", "State", "Checks", "Comments"]);
    for review in &reviews {
        table.add_row(vec![
            Cell::new(&review.id),
            Cell::new(&review.title),
            Cell::new(&review.state_name),
            Cell::new(
                review
                    .checks
                    .iter()
                    .map(|check| check.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            Cell::new(review.unresolved_comments.to_string()),
        ]);
    }
    println!("{table}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urls_roundtrip_as_strings() {
        let review = CodeReview {
            id: "1".to_string(),
            title: "t".to_string(),
            url: Url::parse("https://example.com/reviews/1").unwrap(),
            state_name: "Accepted".to_string(),
            state: CodeReviewState::Accepted,
            checks: vec![Check {
                name: "Lint".to_string(),
                url: Some(Url::parse("https://example.com/checks/lint").unwrap()),
                state: CheckState::Pass,
            }],
            unresolved_comments: 0,
        };
        let json = serde_json::to_value(&review).unwrap();
        assert_eq!(json["url"], "https://example.com/reviews/1");
        assert_eq!(json["checks"][0]["url"], "https://example.com/checks/lint");
        let parsed: CodeReview = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.url, review.url);
        assert_eq!(parsed.checks[0].url, review.checks[0].url);
    }
}
