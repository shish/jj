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
use crate::commands::gerrit::client::GerritClient;
use crate::commands::log;
use crate::ui::Ui;
use jj_lib::object_id::ObjectId as _;
use jj_lib::ref_name::RemoteName;
use jj_lib::trailer::parse_description_trailers;

pub async fn cmd_gerrit_log(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    _args: &CrLogArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;

    // Phase 1: Find CR IDs for all changes in the log.
    let change_ids_by_commit = log_commits(ui, &workspace_command)
        .await?
        .into_iter()
        .map(|commit| {
            let change_id = parse_description_trailers(commit.description())
                .into_iter()
                .find(|trailer| trailer.key == "Change-Id")
                .map_or_else(
                    || format!("I{}6a6a6964", commit.change_id().hex()),
                    |trailer| trailer.value,
                );
            (commit.id().clone(), change_id)
        })
        .collect::<BTreeMap<_, _>>();

    // Phase 2: Find CR status for each CR ID.
    let review_by_change_id = if change_ids_by_commit.is_empty() {
        BTreeMap::new()
    } else {
        let client = GerritClient::new(ui, command, remote_name.as_str()).await?;
        let query = format!("owner:self+status:open+project:{}", client.project_id);
        let changes = client.get(&format!(
            "changes/?q={query}&o=SUBMIT_REQUIREMENTS&o=DETAILED_ACCOUNTS"
        ))?;
        let mut review_by_change_id = BTreeMap::new();
        for change in changes.as_array().into_iter().flatten() {
            let change_id = change["change_id"]
                .as_str()
                .ok_or_else(|| user_error("Gerrit change is missing 'change_id'"))?;
            let review = client.parse_cr(change)?;
            review_by_change_id.insert(change_id.to_owned(), review);
        }
        review_by_change_id
    };

    // Phase 3: Run `jj log` with a custom extension for making review
    // data available to the renderer
    let extension =
        CodeReviewTemplateLanguageExtension::new(change_ids_by_commit, review_by_change_id);
    log::cmd_log_with_template_extensions(ui, command, &Default::default(), &[&extension]).await
}
