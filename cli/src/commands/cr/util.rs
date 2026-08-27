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

use jj_lib::commit::Commit;
use jj_lib::repo::Repo as _;
use reqwest::Url;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::cli_util::WorkspaceCommandHelper;
use crate::command_error::CommandError;
use crate::command_error::CommandErrorKind;
use crate::command_error::user_error;
use crate::ui::Ui;
use jj_lib::git;
use jj_lib::ref_name::RemoteName;

/// Resolves the URL of a git remote by name
pub async fn get_remote_url(
    ui: &Ui,
    command: &CommandHelper,
    name: &RemoteName,
) -> Result<gix::url::Url, CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;
    let git_repo = git::get_git_repo(workspace_command.repo().store())?;
    let Some(remote) = git::try_find_active_remote(&git_repo, name)? else {
        return Err(CommandError::new(CommandErrorKind::User, "No remote found"));
    };
    remote
        .url(gix::remote::Direction::Fetch)
        .map(|url| url.clone())
        .ok_or(user_error("No remote URL found"))
}

/// Normalizes a git remote URL into a forge's web/API base URL: `http(s)`
/// URLs simply have their path stripped, while other schemes (e.g. `ssh`)
/// are rewritten to `https://<host>` (dropping user info and port).
pub fn normalize_forge_url(remote_url: &gix::url::Url) -> Result<Url, CommandError> {
    if matches!(
        remote_url.scheme,
        gix::url::Scheme::Http | gix::url::Scheme::Https
    ) {
        let mut url = remote_url.clone();
        url.path = bstr::BString::default();
        Url::parse(&url.to_bstring().to_string())
            .map_err(|e| user_error(format!("Failed to construct forge URL: {e}")))
    } else {
        let host = remote_url
            .host()
            .ok_or_else(|| user_error(format!("Remote URL has no host: {remote_url}")))?;
        Url::parse(&format!("https://{host}"))
            .map_err(|e| user_error(format!("Failed to construct forge URL: {e}")))
    }
}

#[cfg(test)]
mod nfu_tests {
    use super::*;

    #[test]
    fn test_normalize_forge_url_http() {
        let url = gix::url::parse("https://github.com/user/repo.git").unwrap();
        let normalized = normalize_forge_url(&url).unwrap();
        assert_eq!(normalized.as_str(), "https://github.com/");
    }

    #[test]
    fn test_normalize_forge_url_ssh() {
        let url = gix::url::parse("ssh://git@github.com/user/repo.git").unwrap();
        let normalized = normalize_forge_url(&url).unwrap();
        assert_eq!(normalized.as_str(), "https://github.com/");
    }

    #[test]
    fn test_normalize_forge_url_scp() {
        let url = gix::url::parse("git@github.com:user/repo.git").unwrap();
        let normalized = normalize_forge_url(&url).unwrap();
        assert_eq!(normalized.as_str(), "https://github.com/");
    }
}

/// Shared helper for finding which commits are in a `jj log` revset,
/// so that we can pre-emptively fetch the code review status for each,
/// before then calling `jj log` with a custom commit->status formatter.
pub(crate) async fn log_commits(
    ui: &mut Ui,
    workspace_command: &WorkspaceCommandHelper,
) -> Result<Vec<Commit>, CommandError> {
    let revset = workspace_command.settings().get_string("revsets.log")?;
    let commit_ids = workspace_command
        .resolve_some_revsets(ui, &[RevisionArg::from(revset)])
        .await?;
    let mut commits = Vec::with_capacity(commit_ids.len());
    for commit_id in commit_ids {
        commits.push(
            workspace_command
                .repo()
                .store()
                .get_commit_async(&commit_id)
                .await?,
        );
    }
    Ok(commits)
}
