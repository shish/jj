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

use serde_json::json;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::commands::cr::CrAbandonArgs;
use crate::commands::phabricator::client::PhabricatorClient;
use crate::commands::phabricator::client::parse_rev_number;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_phabricator_abandon(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrAbandonArgs,
) -> Result<(), CommandError> {
    let client = PhabricatorClient::new(ui, command, remote_name.as_str()).await?;

    let number = parse_rev_number(&args.identifier)?;
    let object_identifier = format!("D{number}");

    let mut transactions = vec![json!({
        "type": "abandon",
        "value": true,
    })];
    if let Some(message) = &args.message {
        transactions.push(json!({
            "type": "comment",
            "value": message,
        }));
    }

    client.call(
        "differential.revision.edit",
        json!({
            "objectIdentifier": object_identifier,
            "transactions": transactions,
        }),
    )?;

    writeln!(ui.status(), "Abandoned revision D{number}")?;
    Ok(())
}
