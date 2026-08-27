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

use std::path::PathBuf;
use std::process::Command;

use clap_complete::ArgValueCompleter;
use futures::StreamExt as _;
use jj_lib::backend::CommitId;
use jj_lib::commit::Commit;
use jj_lib::matchers::EverythingMatcher;
use jj_lib::object_id::ObjectId as _;
use jj_lib::repo::Repo as _;
use jj_lib::rewrite::merge_commit_trees;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::cli_util::WorkspaceCommandHelper;
use crate::command_error::CommandError;
use crate::command_error::internal_error;
use crate::command_error::user_error;
use crate::command_error::user_error_with_message;
use crate::complete;
use crate::ui::Ui;

/// Run pre-commit hooks on a stack of changes
#[derive(clap::Args, Clone, Debug)]
pub struct GitPreCommitArgs {
    /// Ref to check
    #[arg(short, long, value_name = "REF")]
    #[arg(add = ArgValueCompleter::new(complete::revset_expression_all))]
    pub revision: Option<RevisionArg>,
}

pub async fn cmd_git_pre_commit(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &GitPreCommitArgs,
) -> Result<(), CommandError> {
    let mut workspace_command = command.workspace_helper(ui).await?;
    let pre_commit_hook = workspace_command
        .workspace_root()
        .join(".git/hooks/pre-commit");
    if !pre_commit_hook.exists() {
        writeln!(
            ui.status(),
            "No pre-commit configuration found at {}, skipping",
            pre_commit_hook.display()
        )?;
        return Ok(());
    }

    let selected_revset = args
        .revision
        .clone()
        .unwrap_or_else(|| RevisionArg::from(String::from("stack()")));
    let change_ids: Vec<CommitId> = workspace_command
        .resolve_some_revsets(ui, &[selected_revset])
        .await?
        .into_iter()
        .rev()
        .collect();

    // writeln!(ui.status(), "Running pre-commit hook on {:?}", change_ids)?;

    let original_wc_commit = workspace_command
        .get_wc_commit_id()
        .map(|id| workspace_command.repo().store().get_commit(id))
        .transpose()?;

    let run_result =
        run_pre_commit_stack(ui, &mut workspace_command, &pre_commit_hook, change_ids).await;

    if let Some(original_wc_commit) = &original_wc_commit {
        restore_working_copy(ui, &mut workspace_command, original_wc_commit).await?;
    }

    run_result
}

async fn run_pre_commit_stack(
    ui: &mut Ui,
    workspace_command: &mut WorkspaceCommandHelper,
    pre_commit_hook: &PathBuf,
    change_ids: Vec<CommitId>,
) -> Result<(), CommandError> {
    for (n, change_id) in change_ids.iter().enumerate() {
        let commit = workspace_command
            .repo()
            .store()
            .get_commit_async(change_id)
            .await?;

        if n > 0 {
            println!("{}", "=".repeat(80));
        }

        {
            let mut tx = workspace_command.start_transaction();
            tx.edit(&commit)?;
            tx.finish(
                ui,
                format!("check out {} for git pre-commit", commit.id().hex()),
            )
            .await?;
        }

        let files = changed_existing_files(workspace_command, &commit).await?;
        let descr = commit
            .description()
            .lines()
            .next()
            .filter(|line| !line.is_empty())
            .unwrap_or("(untitled)");
        println!("Checking \"{}\" ({})", descr, commit.change_id());
        println!(
            "Affected files: {}",
            if files.is_empty() {
                "(none)".to_string()
            } else {
                files.join(" ")
            }
        );

        run_cmd_in_workspace(workspace_command, "git", &["add", "--all"])?;
        run_pre_commit_hook(workspace_command, pre_commit_hook)?;
    }

    Ok(())
}

async fn changed_existing_files(
    workspace_command: &WorkspaceCommandHelper,
    commit: &Commit,
) -> Result<Vec<String>, CommandError> {
    let parents = commit.parents().await?;
    let repo: &dyn jj_lib::repo::Repo = workspace_command.repo().as_ref();
    let parent_tree = merge_commit_trees(repo, &parents).await?;
    let mut diff = parent_tree.diff_stream(&commit.tree(), &EverythingMatcher);
    let mut files: Vec<String> = Vec::new();
    while let Some(entry) = diff.next().await {
        let values = entry.values.map_err(internal_error)?;
        if values.after.is_present() {
            let abs_path = entry
                .path
                .to_fs_path_unchecked(workspace_command.workspace_root());
            if abs_path.exists() {
                files.push(workspace_command.format_file_path(&entry.path));
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

async fn restore_working_copy(
    ui: &Ui,
    workspace_command: &mut WorkspaceCommandHelper,
    commit: &Commit,
) -> Result<(), CommandError> {
    let mut tx = workspace_command.start_transaction();
    tx.check_out(commit)?;
    tx.finish(ui, format!("restore working copy {}", commit.id().hex()))
        .await?;
    Ok(())
}

fn run_cmd_in_workspace(
    workspace_command: &WorkspaceCommandHelper,
    program: &str,
    args: &[&str],
) -> Result<(), CommandError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workspace_command.workspace_root())
        .output()
        .map_err(|err| {
            user_error_with_message(format!("Failed to run `{program} {}`", args.join(" ")), err)
        })?;
    if !output.status.success() {
        return Err(user_error(format!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn run_pre_commit_hook(
    workspace_command: &WorkspaceCommandHelper,
    pre_commit_hook: &PathBuf,
) -> Result<(), CommandError> {
    let output = Command::new(pre_commit_hook)
        .current_dir(workspace_command.workspace_root())
        .output()
        .map_err(|err| {
            user_error_with_message(
                format!(
                    "Failed to run pre-commit hook {}",
                    pre_commit_hook.display()
                ),
                err,
            )
        })?;
    if !output.status.success() {
        return Err(user_error(format!(
            "pre-commit checks failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
