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

//! The `demo` forge backend, which serves hard-coded data so that the
//! `jj cr` output formatting can be demonstrated and tested without
//! talking to a real forge.
//!
//! Test with:
//!   `jj cr --forge demo list`
//!   `jj cr --forge demo log`

use std::collections::BTreeMap;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::internal_error;
use crate::commands::cr::CrListArgs;
use crate::commands::cr::CrLogArgs;
use crate::commands::cr::review;
use crate::commands::cr::util::log_commits;
use crate::ui::Ui;

fn fake_review_1() -> review::CodeReview {
    review::CodeReview {
        id: "123".to_string(),
        title: "Demo Review".to_string(),
        url: url::Url::parse("https://example.com/reviews/123").unwrap(),
        state_name: "Accepted".to_string(),
        state: review::CodeReviewState::Accepted,
        checks: vec![
            review::Check {
                name: "Lint".to_string(),
                url: Some(url::Url::parse("https://example.com/checks/lint").unwrap()),
                state: review::CheckState::Pass,
            },
            review::Check {
                name: "Build".to_string(),
                url: Some(url::Url::parse("https://example.com/checks/build").unwrap()),
                state: review::CheckState::InProgress,
            },
        ],
        unresolved_comments: 0,
    }
}

fn fake_review_2() -> review::CodeReview {
    review::CodeReview {
        id: "456".to_string(),
        title: "Demo Review 2".to_string(),
        url: url::Url::parse("https://example.com/reviews/456").unwrap(),
        state_name: "Needs Review".to_string(),
        state: review::CodeReviewState::NeedsReview,
        checks: vec![
            review::Check {
                name: "Lint".to_string(),
                url: Some(url::Url::parse("https://example.com/checks/lint").unwrap()),
                state: review::CheckState::Fail,
            },
            review::Check {
                name: "Build".to_string(),
                url: Some(url::Url::parse("https://example.com/checks/build").unwrap()),
                state: review::CheckState::Other,
            },
            review::Check {
                name: "Deploy".to_string(),
                url: Some(url::Url::parse("https://example.com/checks/deploy").unwrap()),
                state: review::CheckState::Unknown,
            },
        ],
        unresolved_comments: 2,
    }
}

fn fake_review_3() -> review::CodeReview {
    let states = [
        review::CheckState::Pass,
        review::CheckState::Fail,
        review::CheckState::InProgress,
    ];
    let checks: Vec<review::Check> = (0..20)
        .map(|i| review::Check {
            name: format!("Check {i}"),
            url: Some(url::Url::parse("https://example.com/reviews/789").unwrap()),
            state: states[i % states.len()],
        })
        .collect();
    review::CodeReview {
        id: "789".to_string(),
        title: "Demo Review with a longer title to check longer titles".to_string(),
        url: url::Url::parse("https://example.com/reviews/456").unwrap(),
        state_name: "Needs Review".to_string(),
        state: review::CodeReviewState::NeedsReview,
        checks,
        unresolved_comments: 2,
    }
}

pub(crate) fn cmd_list(_ui: &mut Ui, args: &CrListArgs) -> Result<(), CommandError> {
    let reviews = vec![fake_review_1(), fake_review_2(), fake_review_3()];
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&reviews).map_err(internal_error)?
        );
    } else {
        review::display_table(reviews)?;
    }
    Ok(())
}

pub(crate) async fn cmd_log(
    ui: &mut Ui,
    command: &CommandHelper,
    _args: &CrLogArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;

    // Phase 1: Find CR IDs for the first two changes in the log.
    let reviews = vec![fake_review_1(), fake_review_2(), fake_review_3()];
    let review_ids_by_commit = log_commits(ui, &workspace_command)
        .await?
        .into_iter()
        .zip(&reviews)
        .map(|(commit, review)| (commit.id().clone(), review.id.clone()))
        .collect::<BTreeMap<_, _>>();

    // Phase 2: Make the demo CR statuses available to the log template.
    let reviews_by_id = reviews
        .into_iter()
        .map(|review| (review.id.clone(), review))
        .collect::<BTreeMap<_, _>>();

    // Phase 3: Run `jj log` with a custom extension for making review
    // data available to the renderer
    let extension =
        review::CodeReviewTemplateLanguageExtension::new(review_ids_by_commit, reviews_by_id);
    crate::commands::log::cmd_log_with_template_extensions(
        ui,
        command,
        &Default::default(),
        &[&extension],
    )
    .await
}
