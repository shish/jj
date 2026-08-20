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

use std::process::Command;

use jj_lib::backend::CommitId;
use jj_lib::object_id::ObjectId as _;
use jj_lib::ref_name::RemoteName;
use jj_lib::repo::Repo as _;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::commands::cr::CrDownloadArgs;
use crate::commands::gerrit::client::GerritClient;
use crate::commands::gerrit::client::parse_cr_number_with_revision;
use crate::ui::Ui;

pub async fn cmd_gerrit_download(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrDownloadArgs,
) -> Result<(), CommandError> {
    let client = GerritClient::new(ui, command, remote_name.as_str()).await?;
    let (number, revision) = parse_cr_number_with_revision(&args.identifier)?;

    let commit_id = if let Some(rev) = revision {
        writeln!(ui.status(), "Fetching c{number} revision {rev}")?;
        let change_data = client.get(&format!("changes/{number}/revisions/{rev}/commit"))?;
        let commit_hex = change_data["commit"]
            .as_str()
            .ok_or_else(|| user_error(format!("Could not map c{number}/{rev} to a commit ID")))?;
        CommitId::try_from_hex(commit_hex)
            .ok_or_else(|| user_error(format!("Failed to parse commit ID: {commit_hex}")))?
    } else {
        writeln!(ui.status(), "Fetching c{number}")?;
        let change_data = client.get(&format!("changes/{number}?o=CURRENT_REVISION"))?;
        let current_rev = change_data["current_revision"]
            .as_str()
            .ok_or_else(|| user_error(format!("Could not map c{number} to a commit ID")))?;
        CommitId::try_from_hex(current_rev)
            .ok_or_else(|| user_error(format!("Failed to parse commit ID: {current_rev}")))?
    };

    // TODO: find some way to do this with GitFetch library?
    run_git(&["fetch", remote_name.as_str(), &commit_id.hex()])?;

    // git checkout <fetched commit>
    let mut workspace_command = command.workspace_helper(ui).await?;
    let mut tx = workspace_command.start_transaction();
    let commit = tx.repo().store().get_commit_async(&commit_id).await?;
    tx.check_out(&commit)?;
    tx.finish(ui, format!("check out Gerrit change {number}"))
        .await?;

    Ok(())
}

fn run_git(args: &[&str]) -> Result<(), CommandError> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| user_error(format!("Failed to run 'git {}': {e}", args.join(" "))))?;
    if !output.status.success() {
        return Err(user_error(format!(
            "'git {}' failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
