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
pub(crate) mod netrc;
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
    let remote_url = self::util::get_remote_url(ui, command, &remote_name).await?;
    match (&args.subcommand, &forge) {
        // Info
        (CrCommand::Info(_args), _) => {
            println!("Remote name: {remote_name:?}");
            println!("CR backend:  {forge:?}");
            println!("Forge URL:   {}", util::normalize_forge_url(&remote_url)?);
            Ok(())
        }

        // Abandon
        (CrCommand::Abandon(args), ForgeBackend::Forgejo) => {
            let args = crate::commands::forgejo::abandon::AbandonArgs::from_cr_abandon_args(
                args.identifier.clone(),
                args.message.clone(),
            );
            crate::commands::forgejo::abandon::cmd_forgejo_abandon(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Abandon(args), ForgeBackend::Gerrit) => {
            let args = crate::commands::gerrit::abandon::AbandonArgs::from_cr_abandon_args(
                args.identifier.clone(),
                args.message.clone(),
            );
            crate::commands::gerrit::abandon::cmd_gerrit_abandon(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Abandon(args), ForgeBackend::GitHub) => {
            let args = crate::commands::github::abandon::AbandonArgs::from_cr_abandon_args(
                args.identifier.clone(),
                args.message.clone(),
            );
            crate::commands::github::abandon::cmd_github_abandon(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Abandon(args), ForgeBackend::GitLab) => {
            let args = crate::commands::gitlab::abandon::AbandonArgs::from_cr_abandon_args(
                args.identifier.clone(),
                args.message.clone(),
            );
            crate::commands::gitlab::abandon::cmd_gitlab_abandon(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Abandon(args), ForgeBackend::Phabricator) => {
            let args = crate::commands::phabricator::abandon::AbandonArgs::from_cr_abandon_args(
                args.identifier.clone(),
                args.message.clone(),
            );
            crate::commands::phabricator::abandon::cmd_phabricator_abandon(
                ui,
                command,
                &remote_name,
                &args,
            )
            .await
        }
        (CrCommand::Abandon(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr abandon not implemented for {backend:?}"),
        )),

        // Download
        (CrCommand::Download(args), ForgeBackend::Forgejo) => {
            let args = crate::commands::forgejo::download::DownloadArgs::from_cr_download_args(
                args.identifier.clone(),
            );
            crate::commands::forgejo::download::cmd_forgejo_download(
                ui,
                command,
                &remote_name,
                &args,
            )
            .await
        }
        (CrCommand::Download(args), ForgeBackend::Gerrit) => {
            let args = crate::commands::gerrit::download::DownloadArgs::from_cr_download_args(
                args.identifier.clone(),
            );
            crate::commands::gerrit::download::cmd_gerrit_download(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Download(args), ForgeBackend::GitHub) => {
            let args = crate::commands::github::download::DownloadArgs::from_cr_download_args(
                args.identifier.clone(),
            );
            crate::commands::github::download::cmd_github_download(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Download(args), ForgeBackend::GitLab) => {
            let args = crate::commands::gitlab::download::DownloadArgs::from_cr_download_args(
                args.identifier.clone(),
            );
            crate::commands::gitlab::download::cmd_gitlab_download(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Download(args), ForgeBackend::Phabricator) => {
            let args = crate::commands::phabricator::download::DownloadArgs::from_cr_download_args(
                args.identifier.clone(),
            );
            crate::commands::phabricator::download::cmd_phabricator_download(
                ui,
                command,
                &remote_name,
                &args,
            )
            .await
        }
        (CrCommand::Download(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr download not implemented for {backend:?}"),
        )),

        // List
        (CrCommand::List(args), ForgeBackend::Demo) => self::demo::cmd_list(ui, args),
        (CrCommand::List(args), ForgeBackend::Forgejo) => {
            let args = crate::commands::forgejo::list::ListArgs::from_cr_list_args(args.json);
            crate::commands::forgejo::list::cmd_forgejo_list(ui, command, &remote_name, &args).await
        }
        (CrCommand::List(args), ForgeBackend::Gerrit) => {
            let args = crate::commands::gerrit::list::ListArgs::from_cr_list_args(args.json);
            crate::commands::gerrit::list::cmd_gerrit_list(ui, command, &remote_name, &args).await
        }
        (CrCommand::List(args), ForgeBackend::GitHub) => {
            let args = crate::commands::github::list::ListArgs::from_cr_list_args(args.json);
            crate::commands::github::list::cmd_github_list(ui, command, &remote_name, &args).await
        }
        (CrCommand::List(args), ForgeBackend::GitLab) => {
            let args = crate::commands::gitlab::list::ListArgs::from_cr_list_args(args.json);
            crate::commands::gitlab::list::cmd_gitlab_list(ui, command, &remote_name, &args).await
        }
        (CrCommand::List(args), ForgeBackend::Phabricator) => {
            let args = crate::commands::phabricator::list::ListArgs::from_cr_list_args(args.json);
            crate::commands::phabricator::list::cmd_phabricator_list(
                ui,
                command,
                &remote_name,
                &args,
            )
            .await
        }

        // Log
        (CrCommand::Log(args), ForgeBackend::Demo) => self::demo::cmd_log(ui, command, args).await,
        (CrCommand::Log(_args), ForgeBackend::Forgejo) => {
            let args = crate::commands::forgejo::log::LogArgs::from_cr_log_args();
            crate::commands::forgejo::log::cmd_forgejo_log(ui, command, &remote_name, &args).await
        }
        (CrCommand::Log(_args), ForgeBackend::Gerrit) => {
            let args = crate::commands::gerrit::log::LogArgs::from_cr_log_args();
            crate::commands::gerrit::log::cmd_gerrit_log(ui, command, &remote_name, &args).await
        }
        (CrCommand::Log(_args), ForgeBackend::GitHub) => {
            let args = crate::commands::github::log::LogArgs::from_cr_log_args();
            crate::commands::github::log::cmd_github_log(ui, command, &remote_name, &args).await
        }
        (CrCommand::Log(_args), ForgeBackend::GitLab) => {
            let args = crate::commands::gitlab::log::LogArgs::from_cr_log_args();
            crate::commands::gitlab::log::cmd_gitlab_log(ui, command, &remote_name, &args).await
        }
        (CrCommand::Log(_args), ForgeBackend::Phabricator) => {
            let args = crate::commands::phabricator::log::LogArgs::from_cr_log_args();
            crate::commands::phabricator::log::cmd_phabricator_log(ui, command, &remote_name, &args)
                .await
        }

        // Rebase
        (CrCommand::Rebase(args), ForgeBackend::Forgejo) => {
            let args = crate::commands::forgejo::rebase::RebaseArgs::from_cr_rebase_args(
                args.revset.clone(),
                args.all_prs,
                args.all_branches,
            );
            crate::commands::forgejo::rebase::cmd_forgejo_rebase(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Rebase(args), ForgeBackend::Gerrit) => {
            let args = crate::commands::gerrit::rebase::RebaseArgs::from_cr_rebase_args(
                args.revset.clone(),
                args.all_prs,
                args.all_branches,
            );
            crate::commands::gerrit::rebase::cmd_gerrit_rebase(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Rebase(args), ForgeBackend::GitHub) => {
            let args = crate::commands::github::rebase::RebaseArgs::from_cr_rebase_args(
                args.revset.clone(),
                args.all_prs,
                args.all_branches,
            );
            crate::commands::github::rebase::cmd_github_rebase(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Rebase(args), ForgeBackend::GitLab) => {
            let args = crate::commands::gitlab::rebase::RebaseArgs::from_cr_rebase_args(
                args.revset.clone(),
                args.all_prs,
                args.all_branches,
            );
            crate::commands::gitlab::rebase::cmd_gitlab_rebase(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Rebase(args), ForgeBackend::Phabricator) => {
            let args = crate::commands::phabricator::rebase::RebaseArgs::from_cr_rebase_args(
                args.revset.clone(),
                args.all_prs,
                args.all_branches,
            );
            crate::commands::phabricator::rebase::cmd_phabricator_rebase(
                ui,
                command,
                &remote_name,
                &args,
            )
            .await
        }
        (CrCommand::Rebase(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr rebase not implemented for {backend:?}"),
        )),

        // Upload
        (CrCommand::Upload(args), ForgeBackend::Forgejo) => {
            let args = crate::commands::forgejo::upload::UploadArgs::from_cr_upload_args(
                args.revision.clone(),
                args.draft,
                args.message.clone(),
            );
            crate::commands::forgejo::upload::cmd_forgejo_upload(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Upload(args), ForgeBackend::Gerrit) => {
            let client =
                crate::commands::gerrit::client::GerritClient::new(ui, command, &remote_name)
                    .await?;

            writeln!(ui.status(), "Pushing {} to Gerrit", args.revision)?;

            let upload_args = crate::commands::gerrit::upload::UploadArgs::from_cr_upload_args(
                remote_name.as_symbol().to_string(),
                client.default_merge_target.clone(),
                args.revision.clone(),
                args.draft,
                args.message.clone(),
            );

            crate::commands::gerrit::upload::cmd_gerrit_upload(ui, command, &upload_args).await
        }
        (CrCommand::Upload(args), ForgeBackend::GitHub) => {
            let args = crate::commands::github::upload::UploadArgs::from_cr_upload_args(
                args.revision.clone(),
                args.draft,
                args.message.clone(),
            );
            crate::commands::github::upload::cmd_github_upload(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Upload(args), ForgeBackend::GitLab) => {
            let args = crate::commands::gitlab::upload::UploadArgs::from_cr_upload_args(
                args.revision.clone(),
                args.draft,
                args.message.clone(),
            );
            crate::commands::gitlab::upload::cmd_gitlab_upload(ui, command, &remote_name, &args)
                .await
        }
        (CrCommand::Upload(args), ForgeBackend::Phabricator) => {
            let args = crate::commands::phabricator::upload::UploadArgs::from_cr_upload_args(
                args.revision.clone(),
                args.draft,
                args.message.clone(),
            );
            crate::commands::phabricator::upload::cmd_phabricator_upload(
                ui,
                command,
                &remote_name,
                &args,
            )
            .await
        }
        (CrCommand::Upload(_), backend) => Err(CommandError::new(
            CommandErrorKind::User,
            format!("cr upload not implemented for {backend:?}"),
        )),
    }
}
