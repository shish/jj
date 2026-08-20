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

use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use serde_json::json;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::cli_util::WorkspaceCommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::command_error::user_error_with_message;
use crate::commands::cr::CrUploadArgs;
use crate::commands::phabricator::client::PhabricatorClient;
use crate::ui::Ui;
use jj_lib::commit::Commit;
use jj_lib::object_id::ObjectId as _;
use jj_lib::ref_name::RemoteName;
use jj_lib::repo::Repo as _;

static DIFF_REVISION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Differential Revision:.*D(\d+)").expect("valid regex"));

pub async fn cmd_phabricator_upload(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrUploadArgs,
) -> Result<(), CommandError> {
    let client = PhabricatorClient::new(ui, command, remote_name.as_str()).await?;
    let mut workspace_command = command.workspace_helper(ui).await?;

    let selected_revset = args.revision.clone();
    let commit_ids: Vec<_> = workspace_command
        .resolve_some_revsets(ui, &[RevisionArg::from(selected_revset.clone())])
        .await?
        .into_iter()
        .rev()
        .collect();
    workspace_command.check_rewritable(&commit_ids).await?;

    writeln!(
        ui.status(),
        "Pushing revset '{}' to Phabricator ({})",
        selected_revset,
        commit_ids.len()
    )?;

    for commit_id in commit_ids {
        let commit = workspace_command
            .repo()
            .store()
            .get_commit_async(&commit_id)
            .await?;
        push_one(
            ui,
            &mut workspace_command,
            &client,
            &commit,
            args.draft,
            args.message.as_deref(),
            true,
        )
        .await?;
    }

    Ok(())
}

async fn push_one(
    ui: &mut Ui,
    workspace_command: &mut WorkspaceCommandHelper,
    client: &PhabricatorClient,
    commit: &Commit,
    draft: bool,
    _message: Option<&str>,
    pre_commit: bool,
) -> Result<(), CommandError> {
    writeln!(ui.status(), "Pushing {}", commit.change_id())?;

    let mut transactions: Vec<Value> = Vec::new();
    let revision_num = change_to_revision(commit.description());

    let object_identifier = if let Some(rev_num) = revision_num {
        writeln!(ui.status(), "Updating revision D{rev_num}")?;
        Some(revision_to_phid(client, rev_num)?)
    } else {
        writeln!(
            ui.status(),
            "Creating new revision for {}",
            commit.change_id()
        )?;
        transactions.extend(parse_commit_message(client, commit.description())?);
        None
    };

    let diff_phid = push_change_to_differential_via_subprocess(
        ui,
        workspace_command,
        client,
        commit,
        pre_commit,
    )
    .await?;
    transactions.push(json!({"type": "update", "value": diff_phid}));

    let parent_phids = get_parent_phids(ui, workspace_command, client, commit).await?;
    if !parent_phids.is_empty() {
        transactions.push(json!({"type": "parents.set", "value": parent_phids}));
    }

    if draft {
        transactions.push(json!({"type": "draft", "value": true}));
    }

    let mut payload = serde_json::Map::new();
    if let Some(object_identifier) = object_identifier.clone() {
        payload.insert(
            "objectIdentifier".to_string(),
            Value::String(object_identifier),
        );
    }
    payload.insert("transactions".to_string(), Value::Array(transactions));

    let revision_data = client.call("differential.revision.edit", Value::Object(payload))?;
    let revision_id = revision_data["object"]["id"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| revision_data["object"]["id"].as_i64())
        .ok_or_else(|| user_error("Phabricator response missing revision id"))?;
    let revision_url = client
        .forge_url
        .join(&format!("/D{revision_id}"))
        .map_err(|e| user_error(format!("Invalid Differential revision URL: {e}")))?;

    if object_identifier.is_none() {
        let new_message = format!(
            "{}\n\nDifferential Revision: {}",
            commit.description(),
            revision_url
        );
        set_commit_description(workspace_command, ui, commit, new_message).await?;
        println!(
            "Created revision {} for {}",
            revision_url,
            commit.change_id()
        );
    } else {
        println!(
            "Updated revision {} for {}",
            revision_url,
            commit.change_id()
        );
    }

    Ok(())
}

fn parse_commit_message(
    client: &PhabricatorClient,
    description: &str,
) -> Result<Vec<Value>, CommandError> {
    let mut transactions = client.call(
        "differential.parsecommitmessage",
        json!({"corpus": description}),
    )?["transactions"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    for required in ["title", "summary", "testPlan"] {
        let exists = transactions
            .iter()
            .any(|tr| tr["type"].as_str() == Some(required));
        if !exists {
            transactions.push(json!({"type": required, "value": "-"}));
        }
    }

    Ok(transactions)
}

async fn get_parent_phids(
    ui: &Ui,
    workspace_command: &WorkspaceCommandHelper,
    client: &PhabricatorClient,
    commit: &Commit,
) -> Result<Vec<String>, CommandError> {
    let mut phids = Vec::new();
    for parent_id in commit.parent_ids() {
        if !is_mutable_commit(ui, workspace_command, parent_id).await? {
            continue;
        }
        let parent = workspace_command
            .repo()
            .store()
            .get_commit_async(parent_id)
            .await?;
        if let Some(parent_rev) = change_to_revision(parent.description()) {
            phids.push(revision_to_phid(client, parent_rev)?);
        }
    }
    Ok(phids)
}

async fn is_mutable_commit(
    ui: &Ui,
    workspace_command: &WorkspaceCommandHelper,
    commit_id: &jj_lib::backend::CommitId,
) -> Result<bool, CommandError> {
    let expr = workspace_command.parse_revset(
        ui,
        &RevisionArg::from(format!("{} & mutable()", commit_id.hex())),
    )?;
    let mut stream = expr.evaluate_to_commit_ids()?;
    use futures::TryStreamExt as _;
    Ok(stream.try_next().await?.is_some())
}

async fn push_change_to_differential_via_subprocess(
    ui: &Ui,
    workspace_command: &mut WorkspaceCommandHelper,
    client: &PhabricatorClient,
    commit: &Commit,
    pre_commit: bool,
) -> Result<String, CommandError> {
    // Check out target commit so arc can diff it against its parent.
    {
        let mut tx = workspace_command.start_transaction();
        tx.check_out(commit)?;
        tx.finish(ui, format!("check out {} for arc diff", commit.id().hex()))
            .await?;
    }

    if pre_commit && Path::new(".arclint").exists() {
        run_arc_capture(&["lint", "--apply-patches", "--"])?;
    }

    let text = run_arc_capture(&["diff", "HEAD^", "--only", "--json", "--"])?;
    let Some(last_line) = text.lines().last() else {
        return Err(user_error("arc diff did not return JSON output"));
    };
    let diff_json: Value = serde_json::from_str(last_line)
        .map_err(|e| user_error(format!("Failed to parse arc diff JSON output: {e}")))?;
    let diff_id = diff_json["diffID"]
        .as_i64()
        .ok_or_else(|| user_error("arc diff JSON missing diffID"))?;

    let result = client.call(
        "differential.diff.search",
        json!({"constraints": {"ids": [diff_id]}}),
    )?;
    result["data"]
        .as_array()
        .and_then(|data| data.first())
        .and_then(|diff| diff["phid"].as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| user_error("Could not resolve created Differential diff PHID"))
}

pub fn change_to_revision(description: &str) -> Option<i64> {
    DIFF_REVISION_RE
        .captures(description)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
}

fn revision_to_phid(client: &PhabricatorClient, revision: i64) -> Result<String, CommandError> {
    let result = client.call(
        "differential.revision.search",
        json!({"constraints": {"ids": [revision]}}),
    )?;
    result["data"]
        .as_array()
        .and_then(|data| data.first())
        .and_then(|rev| rev["phid"].as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| user_error(format!("Revision D{revision} not found")))
}

fn run_arc_capture(args: &[&str]) -> Result<String, CommandError> {
    let output = Command::new("arc").args(args).output().map_err(|err| {
        user_error_with_message(format!("Failed to run `arc {}`", args.join(" ")), err)
    })?;
    if !output.status.success() {
        return Err(user_error(format!(
            "`arc {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn set_commit_description(
    workspace_command: &mut WorkspaceCommandHelper,
    ui: &Ui,
    commit: &Commit,
    new_description: String,
) -> Result<(), CommandError> {
    let mut tx = workspace_command.start_transaction();
    let target_commit_id = commit.id().clone();

    tx.repo_mut()
        .transform_descendants(vec![target_commit_id.clone()], async |rewriter| {
            let old_commit_id = rewriter.old_commit().id().clone();
            let commit_builder = rewriter.reparent();
            if old_commit_id == target_commit_id {
                commit_builder
                    .set_description(&new_description)
                    .write()
                    .await?;
            } else {
                commit_builder.write().await?;
            }
            Ok(())
        })
        .await?;

    tx.finish(ui, format!("describe commit {}", target_commit_id.hex()))
        .await?;
    Ok(())
}
