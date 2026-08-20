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
use crate::commands::phabricator::client::PhabricatorClient;
use crate::ui::Ui;

use jj_lib::ref_name::RemoteName;
use jj_lib::rewrite::EmptyBehavior;

pub async fn cmd_phabricator_rebase(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrRebaseArgs,
) -> Result<(), CommandError> {
    let client = PhabricatorClient::new(ui, command, remote_name.as_str()).await?;

    let target_revset = rebase_target_revset(args);
    let mut workspace_command = command.workspace_helper(ui).await?;
    let root_ids = resolve_rebase_roots(ui, &workspace_command, &target_revset).await?;

    if root_ids.is_empty() {
        writeln!(ui.status(), "No revisions to rebase.")?;
        return Ok(());
    }
    let base_revset = format!(
        "{}@{}",
        client.default_merge_target,
        remote_name.as_symbol()
    );
    let base_commit = workspace_command
        .resolve_single_rev(ui, &RevisionArg::from(base_revset.clone()))
        .await?;

    let mut plans = Vec::with_capacity(root_ids.len());
    for root_id in &root_ids {
        writeln!(ui.status(), "Rebasing {root_id:.12} onto {base_revset}")?;
        plans.push((root_id.clone(), base_commit.id().clone()));
    }

    execute_rebase_plans(
        ui,
        &mut workspace_command,
        &plans,
        EmptyBehavior::Keep,
        format!(
            "rebase {} commit(s) and their descendants for Phabricator",
            root_ids.len()
        ),
    )
    .await
}
