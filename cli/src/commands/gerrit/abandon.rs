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
use crate::commands::gerrit::client::GerritClient;
use crate::commands::gerrit::client::parse_cr_number;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_gerrit_abandon(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrAbandonArgs,
) -> Result<(), CommandError> {
    let client = GerritClient::new(ui, command, remote_name.as_str()).await?;
    let number = parse_cr_number(&args.identifier)?;

    client._post(
        &format!("changes/{number}/abandon"),
        &json!({
            "message": args
                .message
                .clone()
                .unwrap_or_else(|| "Abandoned via `jj cr abandon`".to_string())
        }),
    )?;

    writeln!(ui.status(), "Abandoned change c{number}")?;
    Ok(())
}
