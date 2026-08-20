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
use crate::complete;
use crate::ui::Ui;
use clap::Subcommand;
use clap_complete::ArgValueCandidates;
use jj_lib::ref_name::RemoteNameBuf;

pub mod client;
pub mod download;
pub mod upload;

/// Interact with GitHub Code Reviews.
#[derive(clap::Args, Clone, Debug)]
#[command(subcommand_required = true)]
pub struct GitHubArgs {
    /// The remote to work with (only named remotes are supported)
    ///
    /// This defaults to the `git.push` setting. If that is not configured, and
    /// if there are multiple remotes, the remote named "origin" will be used.
    #[arg(long)]
    #[arg(add = ArgValueCandidates::new(complete::git_remotes))]
    pub remote: Option<RemoteNameBuf>,

    #[command(subcommand)]
    pub subcommand: GitHubCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum GitHubCommand {
    Download(download::DownloadArgs),
    Upload(upload::UploadArgs),
}

pub async fn cmd_github(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &GitHubArgs,
) -> Result<(), CommandError> {
    let remote_name =
        crate::commands::cr::detect::get_remote_name(ui, command, args.remote.clone()).await?;
    match &args.subcommand {
        GitHubCommand::Download(args) => {
            download::cmd_github_download(ui, command, &remote_name, args).await
        }
        GitHubCommand::Upload(args) => {
            upload::cmd_github_upload(ui, command, &remote_name, args).await
        }
    }
}
