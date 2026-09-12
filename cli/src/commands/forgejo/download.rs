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
use crate::command_error::user_error;
use crate::commands::cr::CrDownloadArgs;
use crate::commands::forgejo::client::ForgejoClient;
use crate::commands::forgejo::client::parse_pr_number;
use crate::git_util::GitSubprocessUi;
use crate::git_util::load_git_import_options;
use crate::git_util::print_git_import_stats;
use crate::ui::Ui;
use jj_lib::git::GitFetch;
use jj_lib::git::GitFetchRefExpression;
use jj_lib::git::GitSettings;
use jj_lib::git::expand_fetch_refspecs;
use jj_lib::ref_name::RefName;
use jj_lib::ref_name::RemoteName;
use jj_lib::repo::Repo as _;
use jj_lib::str_util::StringExpression;

pub async fn cmd_forgejo_download(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrDownloadArgs,
) -> Result<(), CommandError> {
    let client = ForgejoClient::new(ui, command, remote_name.as_str()).await?;
    let pr_number = parse_pr_number(&args.identifier)?;

    writeln!(
        ui.status(),
        "Downloading PR {} from {}",
        pr_number,
        client.forge_url
    )?;

    let pr = client
        .pull_request(pr_number)?
        .ok_or_else(|| user_error(format!("Pull request #{pr_number} was not found")))?;

    // TODO: add the head repository as a remote and pull from there, or pull
    // from the hidden `refs/pull/{pr_number}/head` ref?
    if pr["head"]["repo_id"] != pr["base"]["repo_id"] {
        return Err(user_error("Cross-repository PRs are not yet supported"));
    }
    let branch_name = pr["head"]["ref"]
        .as_str()
        .ok_or_else(|| user_error(format!("PR #{pr_number} is missing its head branch")))?
        .to_string();

    // Fetch just the PR head branch from this remote and import refs.
    let mut workspace_command = command.workspace_helper(ui).await?;
    let mut tx = workspace_command.start_transaction();

    let git_settings = GitSettings::from_settings(tx.settings())?;
    let remote_settings = tx.settings().remote_settings()?;
    let import_options = load_git_import_options(ui, &git_settings, &remote_settings)?;

    let branch_ref = RefName::new(&branch_name);
    let fetch_ref_expr = GitFetchRefExpression {
        bookmark: StringExpression::exact(branch_ref),
        tag: StringExpression::none(),
    };
    let fetch_refspecs = expand_fetch_refspecs(remote_name, fetch_ref_expr)?;

    let mut git_fetch = GitFetch::new(
        tx.repo_mut(),
        git_settings.to_subprocess_options(),
        &import_options,
    )?;
    git_fetch.fetch(
        remote_name,
        fetch_refspecs,
        &mut GitSubprocessUi::new(ui),
        None,
    )?;

    let import_stats = git_fetch.import_refs().await?;
    print_git_import_stats(ui, &tx, &import_stats)?;

    let remote_symbol = branch_ref.to_remote_symbol(remote_name);
    if tx
        .repo()
        .view()
        .get_remote_bookmark(remote_symbol)
        .is_absent()
    {
        return Err(user_error(format!(
            "Branch '{}' was not found on remote {}",
            branch_name,
            remote_name.as_symbol()
        )));
    }
    tx.repo_mut().track_remote_bookmark(remote_symbol).await?;
    tx.finish(
        ui,
        format!(
            "download Forgejo pull request #{} ({})",
            pr_number,
            branch_ref.as_symbol()
        ),
    )
    .await?;

    // jj new <branch>@<remote>
    let mut workspace_command = command.workspace_helper(ui).await?;
    let parent = workspace_command
        .resolve_single_rev(
            ui,
            &RevisionArg::from(format!(
                "{}@{}",
                branch_ref.as_symbol(),
                remote_name.as_symbol()
            )),
        )
        .await?;
    let mut tx = workspace_command.start_transaction();
    let new_commit = tx
        .repo_mut()
        .new_commit(vec![parent.id().clone()], parent.tree())
        .write()
        .await?;
    tx.edit(&new_commit)?;
    tx.finish(
        ui,
        format!("create working copy for Forgejo pull request #{pr_number}"),
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {}
