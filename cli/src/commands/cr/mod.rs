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
use crate::command_error::CommandErrorKind;
pub(crate) mod demo;
pub(crate) mod detect;
pub(crate) mod rebase;
pub(crate) mod review;
pub(crate) mod util;
use self::detect::ForgeBackend;
use crate::complete;
use crate::ui::Ui;
use clap_complete::ArgValueCandidates;
use jj_lib::ref_name::RemoteNameBuf;

/// Manage code reviews
#[derive(clap::Args, Clone, Debug)]
#[command(subcommand_required = true)]
pub struct CrArgs {
    /// The remote to work with (only named remotes are supported)
    ///
    /// This defaults to the `git.push` setting. If that is not configured, and
    /// if there are multiple remotes, the remote named "origin" will be used.
    #[arg(long)]
    #[arg(add = ArgValueCandidates::new(complete::git_remotes))]
    pub remote: Option<RemoteNameBuf>,

    /// The forge backend to work with
    ///
    /// This defaults to the `cr.forge` setting, or if neither is set, will
    /// try to detect the backend from the remote URL.
    #[arg(long, value_enum)]
    pub forge: Option<ForgeBackend>,

    #[command(subcommand)]
    pub subcommand: CrCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum CrCommand {
    Abandon(CrAbandonArgs),
    Download(CrDownloadArgs),
    Info(CrInfoArgs),
    List(CrListArgs),
    Log(CrLogArgs),
    Rebase(CrRebaseArgs),
    Upload(CrUploadArgs),
}

/// Abandon/close a PR/CR/Diff on the forge
#[derive(clap::Args, Clone, Debug)]
pub struct CrAbandonArgs {
    /// PR/Diff/CR ID
    pub identifier: String,

    /// Optional message to include when abandoning/closing the review
    #[arg(short, long)]
    pub message: Option<String>,
}

/// Download a PR/CR/Diff from the forge
#[derive(clap::Args, Clone, Debug)]
pub struct CrDownloadArgs {
    /// PR/Diff/CR ID
    pub identifier: String,
}

/// Display information about a forge
#[derive(clap::Args, Clone, Debug)]
pub struct CrInfoArgs {}

/// List my open PRs/CRs/Diffs for the current project
#[derive(clap::Args, Clone, Debug)]
pub struct CrListArgs {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Run `jj log` with annotated extra output for code review status
#[derive(clap::Args, Clone, Debug)]
pub struct CrLogArgs {}

/// Pull from remote and rebase current stack
#[derive(clap::Args, Clone, Debug)]
pub struct CrRebaseArgs {
    /// Revset to rebase
    #[arg(value_name = "REVSET")]
    pub revset: Option<String>,

    /// Rebase all branches that have an associated CR; skip branches without one
    #[arg(long, short)]
    pub all_prs: bool,

    /// Rebase all branches; use the default merge target for those without a CR
    #[arg(long, short = 'A')]
    pub all_branches: bool,
}

/// Upload current stack to the forge
#[derive(clap::Args, Clone, Debug)]
pub struct CrUploadArgs {
    /// Ref to push
    #[arg(short, long, value_name = "REF", default_value = "stack()")]
    pub revision: String,

    /// Create as a draft/WIP
    #[arg(long, visible_alias = "wip")]
    pub draft: bool,

    /// Commit/PR message
    #[arg(short, long)]
    pub message: Option<String>,
}

/// Dispatch `cr` subcommands to the appropriate backend.
/// eg. in a repo whose remote URL is `https://github.com/foo/bar`,
/// `cr list` will list all PRs for the `foo/bar` repo on GitHub.
pub async fn cmd_cr(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &CrArgs,
) -> Result<(), CommandError> {
    let forge: ForgeBackend = self::detect::get_forge(ui, command, args).await?;
    let remote_name = self::detect::get_remote_name(ui, command, args.remote.clone()).await?;
    let remote_url = self::detect::get_remote_url(ui, command, &remote_name).await?;
    match (&args.subcommand, &forge) {
        // Info
        (CrCommand::Info(_args), _) => {
            println!("Remote name: {remote_name:?}");
            println!("CR backend:  {forge:?}");
            println!("Forge URL:   {}", util::normalize_forge_url(&remote_url)?);
            Ok(())
        }
        // Abandon
        (CrCommand::Abandon(args), ForgeBackend::GitHub) => {
            crate::commands::github::abandon::cmd_github_abandon(ui, command, &remote_name, args)
                .await
        }
        (CrCommand::Abandon(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr abandon not implemented for {backend:?}"),
        )),
        // Download
        (CrCommand::Download(args), ForgeBackend::GitHub) => {
            crate::commands::github::download::cmd_github_download(ui, command, &remote_name, args)
                .await
        }
        (CrCommand::Download(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr download not implemented for {backend:?}"),
        )),
        // List
        (CrCommand::List(args), ForgeBackend::Demo) => self::demo::cmd_list(ui, args),
        (CrCommand::List(args), ForgeBackend::GitHub) => {
            crate::commands::github::list::cmd_github_list(ui, command, &remote_name, args).await
        }
        // Log
        (CrCommand::Log(args), ForgeBackend::Demo) => self::demo::cmd_log(ui, command, args).await,
        (CrCommand::Log(args), ForgeBackend::GitHub) => {
            crate::commands::github::log::cmd_github_log(ui, command, &remote_name, args).await
        }
        // Rebase
        (CrCommand::Rebase(args), ForgeBackend::GitHub) => {
            crate::commands::github::rebase::cmd_github_rebase(ui, command, &remote_name, args)
                .await
        }
        (CrCommand::Rebase(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr rebase not implemented for {backend:?}"),
        )),
        // Upload
        (CrCommand::Upload(args), ForgeBackend::GitHub) => {
            crate::commands::github::upload::cmd_github_upload(ui, command, &remote_name, args)
                .await
        }
        (CrCommand::Upload(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr upload not implemented for {backend:?}"),
        )),
    }
}
