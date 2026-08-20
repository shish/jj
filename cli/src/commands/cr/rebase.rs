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

use crate::cli_util::RevisionArg;
use crate::cli_util::WorkspaceCommandHelper;
use crate::command_error::CommandError;
use crate::commands::cr::CrRebaseArgs;
use crate::ui::Ui;
use jj_lib::backend::CommitId;
use jj_lib::rewrite::EmptyBehavior;
use jj_lib::rewrite::MoveCommitsLocation;
use jj_lib::rewrite::MoveCommitsTarget;
use jj_lib::rewrite::RebaseOptions;
use jj_lib::rewrite::RewriteRefsOptions;
use jj_lib::rewrite::move_commits;

/// Figure out if we're rebasing all branches, a specific branch,
/// or the current branch
pub(crate) fn rebase_target_revset(args: &CrRebaseArgs) -> String {
    if args.all_prs || args.all_branches {
        "mutable()".to_string()
    } else if let Some(revset) = &args.revset {
        revset.clone()
    } else {
        "@".to_string()
    }
}

/// Find the root commit(s) for whichever branches we've been asked to rebase
pub(crate) async fn resolve_rebase_roots(
    ui: &Ui,
    workspace_command: &WorkspaceCommandHelper,
    target_revset: &str,
) -> Result<Vec<CommitId>, CommandError> {
    let roots_revset = format!("roots(mutable()::{target_revset})");
    let roots = workspace_command
        .resolve_revsets_ordered(ui, &[RevisionArg::from(roots_revset)])
        .await?;
    let root_ids: Vec<CommitId> = roots.into_iter().collect();
    if !root_ids.is_empty() {
        workspace_command.check_rewritable(&root_ids).await?;
    }
    Ok(root_ids)
}

/// Having figured out a bunch of branch-roots, and a bunch of target commits,
/// execute the rebase plans.
pub(crate) async fn execute_rebase_plans(
    ui: &mut Ui,
    workspace_command: &mut WorkspaceCommandHelper,
    plans: &[(CommitId, CommitId)],
    empty_behavior: EmptyBehavior,
    tx_description: String,
) -> Result<(), CommandError> {
    let mut tx = workspace_command.start_transaction();
    let rebase_options = RebaseOptions {
        empty: empty_behavior,
        rewrite_refs: RewriteRefsOptions {
            delete_abandoned_bookmarks: false,
        },
        simplify_ancestor_merge: false,
    };

    for (root_id, base_id) in plans {
        let location = MoveCommitsLocation {
            new_parent_ids: vec![base_id.clone()],
            new_child_ids: vec![],
            target: MoveCommitsTarget::Roots(vec![root_id.clone()]),
        };
        move_commits(tx.repo_mut(), &location, &rebase_options).await?;
    }

    tx.finish(ui, tx_description).await?;
    Ok(())
}
