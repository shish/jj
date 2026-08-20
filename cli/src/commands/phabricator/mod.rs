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
use crate::commands::cr::{
    CrAbandonArgs, CrDownloadArgs, CrListArgs, CrLogArgs, CrRebaseArgs, CrUploadArgs,
};
use crate::complete;
use crate::ui::Ui;
use clap::Subcommand;
use clap_complete::ArgValueCandidates;
use jj_lib::ref_name::RemoteNameBuf;

pub mod abandon;
pub mod client;
pub mod download;
pub mod list;
pub mod log;
pub mod rebase;
pub mod upload;

/// Interact with Gerrit Code Review.
#[derive(clap::Args, Clone, Debug)]
#[command(subcommand_required = true)]
pub struct PhabricatorArgs {
    /// The remote to work with (only named remotes are supported)
    ///
    /// This defaults to the `git.push` setting. If that is not configured, and
    /// if there are multiple remotes, the remote named "origin" will be used.
    #[arg(long)]
    #[arg(add = ArgValueCandidates::new(complete::git_remotes))]
    pub remote: Option<RemoteNameBuf>,

    #[command(subcommand)]
    pub subcommand: PhabricatorCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum PhabricatorCommand {
    Abandon(CrAbandonArgs),
    Download(CrDownloadArgs),
    List(CrListArgs),
    Log(CrLogArgs),
    Rebase(CrRebaseArgs),
    Upload(CrUploadArgs),
}

pub async fn cmd_phabricator(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &PhabricatorArgs,
) -> Result<(), CommandError> {
    let remote_name =
        crate::commands::cr::detect::get_remote_name(ui, command, args.remote.clone()).await?;
    match &args.subcommand {
        PhabricatorCommand::Abandon(args) => {
            abandon::cmd_phabricator_abandon(ui, command, &remote_name, args).await
        }
        PhabricatorCommand::Download(args) => {
            download::cmd_phabricator_download(ui, command, &remote_name, args).await
        }
        PhabricatorCommand::List(args) => {
            list::cmd_phabricator_list(ui, command, &remote_name, args).await
        }
        PhabricatorCommand::Log(args) => {
            log::cmd_phabricator_log(ui, command, &remote_name, args).await
        }
        PhabricatorCommand::Rebase(args) => {
            rebase::cmd_phabricator_rebase(ui, command, &remote_name, args).await
        }
        PhabricatorCommand::Upload(args) => {
            upload::cmd_phabricator_upload(ui, command, &remote_name, args).await
        }
    }
}
