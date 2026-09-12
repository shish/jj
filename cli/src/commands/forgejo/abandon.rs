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
use crate::commands::forgejo::client::ForgejoClient;
use crate::commands::forgejo::client::parse_pr_number;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_forgejo_abandon(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrAbandonArgs,
) -> Result<(), CommandError> {
    let client = ForgejoClient::new(ui, command, remote_name.as_str()).await?;
    let pr_number = parse_pr_number(&args.identifier)?;

    let pr = client
        .pull_request(pr_number)?
        .ok_or_else(|| user_error(format!("Pull request #{pr_number} was not found")))?;
    let title = pr["title"].as_str().unwrap_or_default().to_string();

    if pr["state"].as_str() == Some("closed") {
        writeln!(ui.status(), "Pull request #{pr_number} is already closed")?;
        return Ok(());
    }

    // A comment has to be added before closing, as Forgejo doesn't allow
    // commenting on a closed pull request.
    if let Some(message) = &args.message {
        client.post(
            &client.repo_path(&format!("issues/{pr_number}/comments")),
            json!({ "body": message }),
        )?;
    }

    client.patch(
        &client.repo_path(&format!("pulls/{pr_number}")),
        json!({ "state": "closed" }),
    )?;

    writeln!(ui.status(), "Closed pull request #{pr_number} ({title})")?;
    Ok(())
}
