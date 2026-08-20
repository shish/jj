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

use std::collections::HashMap;
use std::collections::HashSet;

use serde_json::Value;
use serde_json::json;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::internal_error;
use crate::command_error::user_error;
use crate::commands::cr::CrListArgs;
use crate::commands::cr::review;
use crate::commands::phabricator::client::PhabricatorClient;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_phabricator_list(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrListArgs,
) -> Result<(), CommandError> {
    let client = PhabricatorClient::new(ui, command, remote_name.as_str()).await?;
    writeln!(
        ui.status(),
        "Listing CRs on {} ({})",
        client.forge_url,
        client.project_id
    )?;

    let revs = my_open_crs(&client)?;
    let diff_phids: Vec<String> = revs
        .iter()
        .filter_map(|rev| rev["fields"]["diffPHID"].as_str().map(ToOwned::to_owned))
        .collect();
    let rev_ids: Vec<i64> = revs.iter().filter_map(|rev| rev["id"].as_i64()).collect();

    let checks_by_diff = get_checks(&client, &diff_phids)?;
    let unresolved_by_rev = get_unresolved_counts(&client, &rev_ids)?;

    let reviews = revs
        .iter()
        .map(|rev| parse_cr(rev, &checks_by_diff, &unresolved_by_rev))
        .collect::<Result<Vec<_>, _>>()?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&reviews).map_err(internal_error)?
        );
    } else {
        review::display_table(reviews)?;
    }

    Ok(())
}

fn my_open_crs(client: &PhabricatorClient) -> Result<Vec<Value>, CommandError> {
    let me = client.call("user.whoami", Value::Null)?;
    let my_phid = me["phid"]
        .as_str()
        .ok_or_else(|| user_error("Phabricator user.whoami missing phid"))?;

    let repo_phid = callsign_to_phid(client, &client.project_id)?;
    let result = client.call(
        "differential.revision.search",
        json!({
            "constraints": {
                "authorPHIDs": [my_phid],
                "statuses": [
                    "draft",
                    "needs-review",
                    "needs-revision",
                    "accepted",
                    "changes-planned"
                ]
            }
        }),
    )?;
    let revisions = result["data"].as_array().cloned().unwrap_or_default();
    let diff_phids: Vec<_> = revisions
        .iter()
        .filter_map(|revision| revision["fields"]["diffPHID"].as_str())
        .collect();
    if diff_phids.is_empty() {
        return Ok(vec![]);
    }

    let result = client.call(
        "differential.diff.search",
        json!({"constraints": {"phids": diff_phids}}),
    )?;
    let revision_phids: HashSet<_> = result["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diff| diff["fields"]["repositoryPHID"].as_str() == Some(repo_phid.as_str()))
        .filter_map(|diff| diff["fields"]["revisionPHID"].as_str())
        .collect();

    Ok(revisions
        .into_iter()
        .filter(|revision| {
            revision["phid"]
                .as_str()
                .is_some_and(|phid| revision_phids.contains(phid))
        })
        .collect())
}

fn callsign_to_phid(client: &PhabricatorClient, callsign: &str) -> Result<String, CommandError> {
    let result = client.call(
        "diffusion.repository.search",
        json!({"constraints": {"callsigns": [callsign]}}),
    )?;
    result["data"]
        .as_array()
        .and_then(|data| data.first())
        .and_then(|repo| repo["phid"].as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            user_error(format!(
                "Could not resolve repository callsign '{callsign}'"
            ))
        })
}

type PhId = String;
type CheckResults = HashMap<PhId, Vec<(String, String, reqwest::Url)>>;

pub(crate) fn get_checks(
    client: &PhabricatorClient,
    diff_phids: &[String],
) -> Result<CheckResults, CommandError> {
    let diff_phids: Vec<String> = diff_phids
        .iter()
        .filter(|phid| !phid.is_empty())
        .cloned()
        .collect();
    if diff_phids.is_empty() {
        return Ok(HashMap::new());
    }

    let buildables = client.call(
        "harbormaster.buildable.search",
        json!({"constraints": {"objectPHIDs": diff_phids}}),
    )?["data"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if buildables.is_empty() {
        return Ok(HashMap::new());
    }

    let mut buildable_to_diff: HashMap<String, String> = HashMap::new();
    for buildable in &buildables {
        if let (Some(buildable_phid), Some(diff_phid)) = (
            buildable["phid"].as_str(),
            buildable["fields"]["objectPHID"].as_str(),
        ) {
            buildable_to_diff.insert(buildable_phid.to_string(), diff_phid.to_string());
        }
    }

    let buildables: Vec<&str> = buildable_to_diff.keys().map(String::as_str).collect();
    let builds = client.call(
        "harbormaster.build.search",
        json!({"constraints": {"buildables": buildables}}),
    )?["data"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut checks: CheckResults = HashMap::new();
    for build in &builds {
        let Some(buildable_phid) = build["fields"]["buildablePHID"].as_str() else {
            continue;
        };
        let Some(diff_phid) = buildable_to_diff.get(buildable_phid) else {
            continue;
        };

        let name = build["fields"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let status = build["fields"]["buildStatus"]["value"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let build_id = build["id"].as_i64().unwrap_or_default();
        let url = client
            .forge_url
            .join(&format!("/harbormaster/build/{build_id}/"))
            .map_err(|e| user_error(format!("Invalid Harbormaster build URL: {e}")))?;

        checks
            .entry(diff_phid.clone())
            .or_default()
            .push((name, status, url));
    }

    Ok(checks)
}

pub(crate) fn get_unresolved_counts(
    client: &PhabricatorClient,
    revision_nums: &[i64],
) -> Result<HashMap<i64, i64>, CommandError> {
    let mut counts = HashMap::new();
    for rev_num in revision_nums {
        let txns = client.call(
            "transaction.search",
            json!({"objectIdentifier": format!("D{rev_num}")}),
        )?["data"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        for txn in txns {
            if txn["type"].as_str() == Some("inline")
                && txn["fields"]["isDone"].as_bool() == Some(false)
            {
                *counts.entry(*rev_num).or_insert(0) += 1;
            }
        }
    }
    Ok(counts)
}

pub(crate) fn parse_cr(
    rev: &Value,
    checks_by_diff: &HashMap<String, Vec<(String, String, reqwest::Url)>>,
    unresolved_by_rev: &HashMap<i64, i64>,
) -> Result<review::CodeReview, CommandError> {
    let id = rev["id"]
        .as_i64()
        .ok_or_else(|| user_error("Phabricator revision missing id"))?;
    let diff_phid = rev["fields"]["diffPHID"].as_str().unwrap_or_default();

    let state_name = rev["fields"]["status"]["name"]
        .as_str()
        .unwrap_or("Needs Review")
        .to_string();
    let state = match state_name.as_str() {
        "Draft" | "Changes Planned" => review::CodeReviewState::Draft,
        "Rejected" => review::CodeReviewState::Rejected,
        "Needs Review" => review::CodeReviewState::NeedsReview,
        "Accepted" => review::CodeReviewState::Accepted,
        "Closed" | "Abandoned" => review::CodeReviewState::Closed,
        _ => review::CodeReviewState::Other,
    };

    let checks = checks_by_diff
        .get(diff_phid)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, status, url)| review::Check {
            name,
            state: match status.as_str() {
                "passed" => review::CheckState::Pass,
                "failed" | "aborted" | "error" | "deadlocked" => review::CheckState::Fail,
                _ => review::CheckState::Other,
            },
            url: Some(url),
        })
        .collect();

    let url = rev["fields"]["uri"]
        .as_str()
        .ok_or_else(|| user_error("Phabricator revision missing uri"))?;
    let url =
        reqwest::Url::parse(url).map_err(|e| user_error(format!("Invalid revision URL: {e}")))?;

    Ok(review::CodeReview {
        id: format!("D{id}"),
        title: rev["fields"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        url,
        state_name,
        state,
        checks,
        unresolved_comments: unresolved_by_rev.get(&id).copied().unwrap_or_default(),
    })
}
