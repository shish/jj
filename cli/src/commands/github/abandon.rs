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

use serde_json::json;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::commands::cr::CrAbandonArgs;
use crate::commands::github::client::GitHubClient;
use crate::commands::github::client::parse_pr_number;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_github_abandon(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrAbandonArgs,
) -> Result<(), CommandError> {
    let client = GitHubClient::new(ui, command, remote_name.as_str()).await?;
    let pr_number = parse_pr_number(&args.identifier)?;

    let data = client.graphql(
        r#"
            query GetPullRequestId($owner: String!, $name: String!, $number: Int!) {
                repository(owner: $owner, name: $name) {
                    pullRequest(number: $number) {
                        id
                        number
                        state
                        title
                    }
                }
            }
        "#,
        json!({
            "owner": client.repo_owner,
            "name": client.repo_name,
            "number": pr_number,
        }),
    )?;

    let pr = &data["repository"]["pullRequest"];
    if pr.is_null() {
        return Err(user_error(format!(
            "Pull request #{pr_number} was not found"
        )));
    }
    let pr_id = pr["id"]
        .as_str()
        .ok_or_else(|| user_error("GitHub pullRequest is missing id"))?;
    let title = pr["title"].as_str().unwrap_or_default();

    if pr["state"].as_str() == Some("CLOSED") {
        writeln!(ui.status(), "Pull request #{pr_number} is already closed")?;
        return Ok(());
    }

    client.graphql(
        r#"
            mutation ClosePullRequest($input: ClosePullRequestInput!) {
                closePullRequest(input: $input) {
                    pullRequest {
                        number
                        state
                    }
                }
            }
        "#,
        json!({
            "input": {
                "pullRequestId": pr_id,
            }
        }),
    )?;

    if let Some(message) = &args.message {
        client.graphql(
            r#"
                mutation AddComment($input: AddCommentInput!) {
                    addComment(input: $input) {
                        subject {
                            id
                        }
                    }
                }
            "#,
            json!({
                "input": {
                    "subjectId": pr_id,
                    "body": message,
                }
            }),
        )?;
    }

    writeln!(ui.status(), "Closed pull request #{pr_number} ({title})")?;
    Ok(())
}
