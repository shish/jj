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

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::CommandError;
use crate::commands::cr::CrRebaseArgs;
use crate::commands::cr::rebase::execute_rebase_plans;
use crate::commands::cr::rebase::rebase_target_revset;
use crate::commands::cr::rebase::resolve_rebase_roots;
use crate::commands::gerrit::client::GerritClient;
use crate::ui::Ui;
use jj_lib::backend::CommitId;
use jj_lib::object_id::ObjectId as _;
use jj_lib::ref_name::RemoteName;
use jj_lib::repo::Repo as _;
use jj_lib::rewrite::EmptyBehavior;

pub async fn cmd_gerrit_rebase(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrRebaseArgs,
) -> Result<(), CommandError> {
    let client = GerritClient::new(ui, command, remote_name.as_str()).await?;

    let target_revset = rebase_target_revset(args);
    let mut workspace_command = command.workspace_helper(ui).await?;
    let root_ids = resolve_rebase_roots(ui, &workspace_command, &target_revset).await?;

    if root_ids.is_empty() {
        println!("No revisions to rebase.");
        return Ok(());
    }
    let mut plans: Vec<(CommitId, CommitId)> = Vec::new();
    for root_id in &root_ids {
        let root_commit = workspace_command
            .repo()
            .store()
            .get_commit_async(root_id)
            .await?;
        let gerrit_change_id = format!("I{}6a6a6964", root_commit.change_id().hex());

        let merge_target = get_gerrit_branch(&client, &gerrit_change_id, remote_name)?;

        let base_revset = if let Some(branch_at_remote) = merge_target {
            writeln!(
                ui.status(),
                "Found CR branch '{}' for {:.12}, rebasing onto {}",
                branch_at_remote,
                root_commit.change_id(),
                branch_at_remote
            )?;
            branch_at_remote
        } else if args.all_prs {
            writeln!(
                ui.status(),
                "No CR found for {:.12}, skipping",
                root_commit.change_id()
            )?;
            continue;
        } else {
            let base = format!(
                "{}@{}",
                client.default_merge_target,
                remote_name.as_symbol()
            );
            writeln!(
                ui.status(),
                "No CR found for {:.12}, rebasing onto default target {}",
                root_commit.change_id(),
                base
            )?;
            base
        };

        println!(
            "Rebasing {:.12} onto {}",
            root_commit.change_id(),
            base_revset
        );

        let base_commit = workspace_command
            .resolve_single_rev(ui, &RevisionArg::from(base_revset))
            .await?;
        plans.push((root_id.clone(), base_commit.id().clone()));
    }

    if plans.is_empty() {
        writeln!(ui.status(), "No revisions selected for rebasing.")?;
        return Ok(());
    }

    execute_rebase_plans(
        ui,
        &mut workspace_command,
        &plans,
        EmptyBehavior::Keep,
        format!(
            "rebase {} commit(s) and their descendants for Gerrit",
            plans.len()
        ),
    )
    .await
}

fn get_gerrit_branch(
    client: &GerritClient,
    gerrit_change_id: &str,
    remote_name: &RemoteName,
) -> Result<Option<String>, CommandError> {
    let Some(change_data) = client.get_optional(&format!("changes/{gerrit_change_id}"))? else {
        return Ok(None);
    };
    Ok(change_data["branch"]
        .as_str()
        .map(|branch| format!("{branch}@{}", remote_name.as_symbol())))
}
