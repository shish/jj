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

use std::collections::BTreeMap;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::commands::cr::CrLogArgs;
use crate::commands::cr::review::CodeReviewTemplateLanguageExtension;
use crate::commands::cr::util::log_commits;
use crate::commands::log;
use crate::commands::phabricator::client::PhabricatorClient;
use crate::commands::phabricator::list;
use crate::commands::phabricator::upload::change_to_revision;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;
use serde_json::json;

pub async fn cmd_phabricator_log(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    _args: &CrLogArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;
    // Phase 1: Find CR IDs for all changes in the log.
    let revision_nums_by_commit = log_commits(ui, &workspace_command)
        .await?
        .into_iter()
        .filter_map(|commit| {
            change_to_revision(commit.description()).map(|revision| (commit.id().clone(), revision))
        })
        .collect::<BTreeMap<_, _>>();

    // Phase 2: Find CR status for each CR ID.
    let review_by_revision = if revision_nums_by_commit.is_empty() {
        BTreeMap::new()
    } else {
        let client = PhabricatorClient::new(ui, command, remote_name.as_str()).await?;
        let revisions = client.call(
            "differential.revision.search",
            json!({"constraints": {"ids": revision_nums_by_commit.values().collect::<Vec<_>>()}}),
        )?["data"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let diff_phids = revisions
            .iter()
            .filter_map(|revision| {
                revision["fields"]["diffPHID"]
                    .as_str()
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        let revision_ids = revisions
            .iter()
            .filter_map(|revision| revision["id"].as_i64())
            .collect::<Vec<_>>();
        let checks_by_diff = list::get_checks(&client, &diff_phids)?;
        let unresolved_by_revision = list::get_unresolved_counts(&client, &revision_ids)?;
        revisions
            .iter()
            .map(|revision| {
                let revision_id = revision["id"]
                    .as_i64()
                    .ok_or_else(|| user_error("Phabricator revision missing id"))?;
                let review = list::parse_cr(revision, &checks_by_diff, &unresolved_by_revision)?;
                Ok((format!("D{revision_id}"), review))
            })
            .collect::<Result<BTreeMap<_, _>, CommandError>>()?
    };
    let revision_identifiers_by_commit = revision_nums_by_commit
        .into_iter()
        .map(|(commit_id, revision)| (commit_id, format!("D{revision}")))
        .collect();

    // Phase 3: Run `jj log` with a custom extension for making review
    // data available to the renderer
    let extension = CodeReviewTemplateLanguageExtension::new(
        revision_identifiers_by_commit,
        review_by_revision,
    );
    log::cmd_log_with_template_extensions(ui, command, &Default::default(), &[&extension]).await
}
