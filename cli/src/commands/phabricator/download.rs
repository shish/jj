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

use std::process::Command;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::command_error::user_error_with_message;
use crate::commands::phabricator::client::parse_rev_number;
use crate::ui::Ui;
use jj_lib::ref_name::RemoteName;

/// Download a PR/CR/Diff from the forge
#[derive(clap::Args, Clone, Debug)]
pub struct DownloadArgs {
    /// PR/Diff/CR ID
    identifier: String,
}

impl DownloadArgs {
    pub fn from_cr_download_args(identifier: String) -> Self {
        Self { identifier }
    }
}

pub async fn cmd_phabricator_download(
    ui: &mut Ui,
    _command: &CommandHelper,
    _remote_name: &RemoteName,
    args: &DownloadArgs,
) -> Result<(), CommandError> {
    let number = parse_rev_number(&args.identifier)?;
    let revision_id = format!("D{number}");

    writeln!(ui.status(), "Checking out Phabricator diff {revision_id}")?;
    run_arc(&["patch", &revision_id])
}

fn run_arc(args: &[&str]) -> Result<(), CommandError> {
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
    Ok(())
}
