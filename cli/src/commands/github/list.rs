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

use serde_json::Value;
use serde_json::json;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::internal_error;
use crate::commands::cr::CrListArgs;
use crate::commands::cr::review;
use crate::commands::github::client;
use crate::commands::github::client::GitHubClient;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_github_list(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrListArgs,
) -> Result<(), CommandError> {
    let client = GitHubClient::new(ui, command, remote_name.as_str()).await?;
    writeln!(
        ui.status(),
        "Listing CRs on {} ({}/{})",
        client.forge_url,
        client.repo_owner,
        client.repo_name
    )?;

    let query = format!(
        r#"
            query PullRequestSearch($q: String!, $limit: Int!, $endCursor: String) {{
                search(query: $q, type: ISSUE, first: $limit, after: $endCursor) {{
                    nodes {{
                        ... on PullRequest {{
                            number
                            title
                            state
                            url
                            isDraft
                            {}
                            {}
                            {}
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                        endCursor
                    }}
                }}
            }}
        "#,
        client::STATUS_CHECK_FIELDS,
        client::REVIEW_FIELDS,
        client::REVIEW_THREAD_FIELDS,
    );

    let mut prs: Vec<Value> = Vec::new();
    let mut end_cursor: Option<String> = None;
    loop {
        let data = client.graphql(
            &query,
            json!({
                "q": format!(
                    "repo:{}/{} author:@me state:open type:pr",
                    client.repo_owner, client.repo_name
                ),
                "limit": 100,
                "endCursor": end_cursor,
            }),
        )?;
        let search = &data["search"];
        if let Some(nodes) = search["nodes"].as_array() {
            prs.extend(nodes.iter().cloned());
        }
        if !search["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false) {
            break;
        }
        end_cursor = search["pageInfo"]["endCursor"].as_str().map(String::from);
    }

    let reviews = prs
        .iter()
        .map(client::parse_cr)
        .collect::<Result<Vec<_>, _>>()?;
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
