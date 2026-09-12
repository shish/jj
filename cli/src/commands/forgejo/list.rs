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
use crate::command_error::CommandError;
use crate::command_error::internal_error;
use crate::commands::cr::CrListArgs;
use crate::commands::cr::review;
use crate::commands::forgejo::client::ForgejoClient;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

pub async fn cmd_forgejo_list(
    ui: &mut Ui,
    command: &CommandHelper,
    remote_name: &RemoteName,
    args: &CrListArgs,
) -> Result<(), CommandError> {
    let client = ForgejoClient::new(ui, command, remote_name.as_str()).await?;
    writeln!(
        ui.status(),
        "Listing CRs on {} ({}/{})",
        client.forge_url,
        client.repo_owner,
        client.repo_name
    )?;

    let me = client.current_user_login()?;
    let pull_requests = client.pull_requests(&[("state", "open"), ("poster", &me)])?;
    let reviews = pull_requests
        .iter()
        // Older instances may ignore the `poster` filter.
        .filter(|pr| pr["user"]["login"].as_str() == Some(me.as_str()))
        .map(|pr| client.fetch_cr(pr))
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

#[cfg(test)]
mod tests {}
