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
use crate::cli_util::WorkspaceCommandHelper;
use crate::command_error::CommandError;
use crate::command_error::CommandErrorKind;
use crate::commands::cr::CrArgs;
use crate::commands::git::get_single_remote;
use crate::ui::Ui;
use clap::ValueEnum as _;
use jj_lib::config::ConfigGetResultExt as _;
use jj_lib::content_hash::ContentHash;
use jj_lib::git;
use jj_lib::ref_name::RemoteName;
use jj_lib::ref_name::RemoteNameBuf;
use jj_lib::repo::Repo as _;
use reqwest::Url;
use std::str::FromStr as _;

const DEFAULT_REMOTE: &RemoteName = RemoteName::new("origin");

#[derive(Clone, ContentHash, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, clap::ValueEnum)]
pub enum ForgeBackend {
    Demo,
    Forgejo,
    Gerrit,
    #[value(name = "github")]
    GitHub,
    Phabricator,
}

pub async fn get_remote_name(
    ui: &Ui,
    command: &CommandHelper,
    name: Option<RemoteNameBuf>,
) -> Result<RemoteNameBuf, CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;
    if let Some(name) = name {
        Ok(name.clone())
    } else {
        Ok(get_default_push_remote(ui, &workspace_command)?.clone())
    }
}

pub async fn get_remote_url(
    ui: &Ui,
    command: &CommandHelper,
    name: &RemoteNameBuf,
) -> Result<Url, CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;
    let git_repo = git::get_git_repo(workspace_command.repo().store())?;
    let Some(remote) = git::try_find_active_remote(&git_repo, &name)? else {
        return Err(CommandError::new(CommandErrorKind::User, "No remote found"));
    };
    let remote_url = remote
        .url(gix::remote::Direction::Fetch)
        .map(|url| url.to_string())
        .unwrap_or_else(|| "<no URL>".into());
    Url::from_str(&remote_url).map_err(|e| CommandError::new(CommandErrorKind::Internal, e))
}

pub async fn get_forge(
    ui: &Ui,
    command: &CommandHelper,
    args: &CrArgs,
) -> Result<ForgeBackend, CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;

    // Check --forge flag
    if let Some(forge) = &args.forge {
        return Ok(forge.clone());
    }

    // Check cr.forge config
    let settings = workspace_command.settings();
    if let Some(forge) = settings.get_string("cr.forge").optional()? {
        return ForgeBackend::from_str(&forge, true).map_err(|err| {
            CommandError::new(
                CommandErrorKind::Config,
                format!("Invalid `cr.forge` value {forge:?}: {err}"),
            )
        });
    }

    // Check remote name
    let remote_name = get_remote_name(ui, command, args.remote.clone()).await?;
    if let Ok(backend) = ForgeBackend::from_str(&remote_name.as_str(), true) {
        return Ok(backend);
    }

    // Check remote url
    let remote_url = get_remote_url(ui, command, &remote_name).await?;
    let remote_host = remote_url.host_str().unwrap_or_default();
    if remote_host.contains("gerrit") {
        return Ok(ForgeBackend::Gerrit);
    } else if remote_host.ends_with("github.com") || remote_host.ends_with("ghe.com") {
        return Ok(ForgeBackend::GitHub);
    } else if remote_host.contains("phab") {
        return Ok(ForgeBackend::Phabricator);
    } else if remote_host.contains("forgejo")
        || remote_host.contains("gitea")
        || remote_host.ends_with("codeberg.org")
    {
        return Ok(ForgeBackend::Forgejo);
    }

    Err(CommandError::new(
        CommandErrorKind::User,
        "No forge backend detected",
    ))
}

// FIXME: Copy-pasted from `git push`
fn get_default_push_remote(
    ui: &Ui,
    workspace_command: &WorkspaceCommandHelper,
) -> Result<RemoteNameBuf, CommandError> {
    let settings = workspace_command.settings();
    if let Some(remote) = settings.get_string("git.push").optional()? {
        Ok(remote.into())
    } else if let Some(remote) = get_single_remote(workspace_command.repo().store().as_ref())? {
        // similar to get_default_fetch_remotes
        if remote != DEFAULT_REMOTE {
            writeln!(
                ui.hint_default(),
                "Working with the only existing remote: {remote}",
                remote = remote.as_symbol()
            )?;
        }
        Ok(remote)
    } else {
        Ok(DEFAULT_REMOTE.to_owned())
    }
}
