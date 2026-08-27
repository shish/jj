// Copyright 2026 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Common types for code review operations.

use std::collections::BTreeMap;
use url::Url;

use crate::backend::CommitId;

/// A minimal, forge-agnostic representation of a code review (PR/CR/Diff),
/// as returned by the `<forge> list` and `<forge> log` commands.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CodeReview {
    /// Forge-specific identifier for the review.
    pub id: String,
    /// Version of the change, eg Gerrit patchset
    pub version: Option<i64>,
    /// The title of the review.
    pub title: String,
    /// The URL of the review.
    pub url: Url,
    /// Forge-specific state name, eg "closed", "abandoned", "trashed"
    pub state_name: String,
    /// The generic state of the review, eg Closed
    pub state: CodeReviewState,
    /// A list of CI/CD checks triggered by the CR
    pub checks: Vec<Check>,
    /// The number of unresolved comments on the review.
    pub unresolved_comments: i64,
}

/// The category of a CR's overall review state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum CodeReviewState {
    /// The review has been accepted.
    Accepted,
    /// The review has been rejected.
    Rejected,
    /// The review needs to be reviewed.
    NeedsReview,
    /// The review is a draft.
    Draft,
    /// The review is blocked, eg automated builds are failing
    Blocked,
    /// The review is private.
    Private,
    /// The review is closed.
    Closed,
    /// The API returned a state which JJ doesn't recognise.
    Other,
}

impl CodeReview {
    /// Maps the state to a label to be used by the template renderer.
    pub fn label(self) -> &'static str {
        match self.state {
            CodeReviewState::Accepted => "accepted",
            CodeReviewState::Rejected => "rejected",
            CodeReviewState::NeedsReview => "needs_review",
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
    /// Name of the check, eg "lint", "test", "typecheck"
    pub name: String,
    /// The URL of the check, if available.
    pub url: Option<Url>,
    /// The state of the check, eg Pass, InProgress, Fail
    pub state: CheckState,
}

impl Check {
    /// Maps the state to a label to be used by the template renderer.
    pub fn label(&self) -> &'static str {
        self.state.label()
    }

    /// Maps the state to an icon to be used by the template renderer.
    pub fn icon(&self) -> &'static str {
        self.state.symbol()
    }
}

/// The state of a single CI check or submit requirement.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum CheckState {
    /// The check passed
    Pass,
    /// The check is in progress
    InProgress,
    /// The check failed
    Fail,
    /// The check is in some known state, but not one that JJ recognises
    Other,
    /// The API didn't tell us anything about the state at all
    Unknown,
}

impl CheckState {
    /// Maps the state to a label to be used by the template renderer.
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "passed",
            Self::Fail => "failed",
            Self::InProgress => "in_progress",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }

    /// Maps the state to an icon to be used by the template renderer.
    pub fn symbol(self) -> &'static str {
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
pub struct CodeReviewLookup {
    identifiers_by_commit: BTreeMap<CommitId, String>,
    reviews_by_identifier: BTreeMap<String, CodeReview>,
}

impl CodeReviewLookup {
    /// Create an empty map for commit-to-review lookup.
    pub fn new(
        identifiers_by_commit: BTreeMap<CommitId, String>,
        reviews_by_identifier: BTreeMap<String, CodeReview>,
    ) -> Self {
        Self {
            identifiers_by_commit,
            reviews_by_identifier,
        }
    }

    /// Find a code review for a given commit ID, if one is known
    pub fn review(&self, commit_id: &CommitId) -> Option<CodeReview> {
        self.identifiers_by_commit
            .get(commit_id)
            .and_then(|identifier| self.reviews_by_identifier.get(identifier))
            .cloned()
    }
}
