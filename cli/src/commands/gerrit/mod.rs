// Copyright 2024 The Jujutsu Authors
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

use std::fmt::Debug;

use clap::Subcommand;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::commands::cr::{CrAbandonArgs, CrDownloadArgs, CrListArgs, CrLogArgs, CrRebaseArgs};
use crate::commands::gerrit;
use crate::complete;
use crate::ui::Ui;
use clap_complete::ArgValueCandidates;
use jj_lib::ref_name::RemoteNameBuf;

/// Interact with Gerrit Code Review.
#[derive(clap::Args, Clone, Debug)]
#[command(subcommand_required = true)]
pub struct GerritArgs {
    /// The remote to work with (only named remotes are supported)
    ///
    /// This defaults to the `git.push` setting. If that is not configured, and
    /// if there are multiple remotes, the remote named "origin" will be used.
    #[arg(long)]
    #[arg(add = ArgValueCandidates::new(complete::git_remotes))]
    pub remote: Option<RemoteNameBuf>,

    #[command(subcommand)]
    pub subcommand: GerritCommand,
}

#[derive(Subcommand, Clone, Debug)]
#[expect(clippy::large_enum_variant)]
pub enum GerritCommand {
    Abandon(CrAbandonArgs),
    Download(CrDownloadArgs),
    List(CrListArgs),
    Log(CrLogArgs),
    Rebase(CrRebaseArgs),
    Upload(gerrit::upload::UploadArgs),
}

pub async fn cmd_gerrit(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &GerritArgs,
) -> Result<(), CommandError> {
    let remote_name =
        crate::commands::cr::detect::get_remote_name(ui, command, args.remote.clone()).await?;
    match &args.subcommand {
        GerritCommand::Abandon(args) => {
            abandon::cmd_gerrit_abandon(ui, command, &remote_name, args).await
        }
        GerritCommand::Download(args) => {
            download::cmd_gerrit_download(ui, command, &remote_name, args).await
        }
        GerritCommand::List(args) => list::cmd_gerrit_list(ui, command, &remote_name, args).await,
        GerritCommand::Log(args) => log::cmd_gerrit_log(ui, command, &remote_name, args).await,
        GerritCommand::Rebase(args) => {
            rebase::cmd_gerrit_rebase(ui, command, &remote_name, args).await
        }
        GerritCommand::Upload(args) => upload::cmd_gerrit_upload(ui, command, args).await,
    }
}

pub mod abandon;
pub mod client;
pub mod download;
pub mod list;
pub mod log;
pub mod rebase;
pub mod upload;
