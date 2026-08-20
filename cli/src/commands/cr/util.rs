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
use std::process::Command;

use crate::cli_util::RevisionArg;
use crate::cli_util::WorkspaceCommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::ui::Ui;

/// Resolves the URL of a git remote, normalizing scp-like syntax (e.g.
/// `git@host:path/to/repo.git`) into a proper `ssh://` URL.
pub fn get_remote_url(remote: &str) -> Result<Url, CommandError> {
    let output = Command::new("git")
        .args(["config", "--get", &format!("remote.{remote}.url")])
        .output()
        .map_err(|e| user_error(format!("Failed to get git remote URL for '{remote}': {e}")))?;
    if !output.status.success() {
        return Err(user_error(format!(
            "Failed to get git remote URL for '{remote}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    normalize_git_url(&raw)
}

pub(crate) fn normalize_git_url(raw: &str) -> Result<Url, CommandError> {
    let normalized = if raw.contains("://") {
        raw.to_string()
    } else if let Some(path) = raw.strip_prefix('/') {
        format!("file:///{path}")
    } else if let Some(colon) = raw.find(':') {
        format!("ssh://{}/{}", &raw[..colon], &raw[colon + 1..])
    } else {
        format!("ssh://{raw}")
    };

    Url::parse(&normalized).map_err(|e| {
        user_error(format!(
            "Failed to parse git remote URL '{normalized}': {e}"
        ))
    })
}

#[cfg(test)]
mod gru_tests {
    use super::*;

    #[test]
    fn test_normalize_git_url() {
        assert_eq!(
            normalize_git_url("https://github.com/user/repo").unwrap(),
            Url::parse("https://github.com/user/repo").unwrap()
        );
        assert_eq!(
            normalize_git_url("git@github.com:user/repo").unwrap(),
            Url::parse("ssh://git@github.com/user/repo").unwrap()
        );
        assert_eq!(
            normalize_git_url("/path/to/repo").unwrap(),
            Url::parse("file:///path/to/repo").unwrap()
        );
        assert_eq!(
            normalize_git_url("ssh://user@host/path/to/repo").unwrap(),
            Url::parse("ssh://user@host/path/to/repo").unwrap()
        );
    }
}

/// Finds the default branch of the given remote, by asking it for the branch
/// its `HEAD` symref points at.
pub fn get_merge_target(remote: &str) -> Result<String, CommandError> {
    let output = Command::new("git")
        .args(["ls-remote", "--symref", remote, "HEAD"])
        .output()
        .map_err(|e| user_error(format!("Failed to find HEAD in remote {remote}: {e}")))?;
    if !output.status.success() {
        return Err(user_error(format!(
            "Failed to find HEAD in remote {remote}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or_default();
    if let Some(rest) = first_line.strip_prefix("ref:")
        && let Some(ref_path) = rest.split_whitespace().next()
        && let Some(branch) = ref_path.strip_prefix("refs/heads/")
    {
        return Ok(branch.to_string());
    }
    Err(user_error(format!(
        "Could not parse git ls-remote output: {first_line}"
    )))
}

/// Reads a `login`/`password` pair for `host` from `~/.netrc`.
pub fn netrc_read(host: &str) -> Option<(String, String)> {
    let home = etcetera::home_dir().ok()?;
    let content = std::fs::read_to_string(home.join(".netrc")).ok()?;
    netrc_parse(&content, host)
}

/// Parses a `.netrc` file for `host` and returns the `login`/`password` pair.
fn netrc_parse(content: &str, host: &str) -> Option<(String, String)> {
    // Strip comments so they don't interfere with tokenization.
    let uncommented: String = content
        .lines()
        .map(|line| line.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let tokens = shlex::split(&uncommented)?;

    let mut matched = false;
    let mut login: Option<String> = None;
    let mut password: Option<String> = None;
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "machine" => {
                if matched {
                    break;
                }
                i += 1;
                matched = tokens.get(i).map(String::as_str) == Some(host);
            }
            "default" => {
                if matched {
                    break;
                }
                matched = true;
            }
            "login" if matched => {
                i += 1;
                login = tokens.get(i).cloned();
            }
            "password" if matched => {
                i += 1;
                password = tokens.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    match (login, password) {
        (Some(login), Some(password)) if !password.is_empty() => Some((login, password)),
        _ => None,
    }
}

#[cfg(test)]
mod netrc_tests {
    use super::*;

    #[test]
    fn test_netrc_parse() {
        let content = "machine github.com login foo password bar";
        assert_eq!(
            netrc_parse(content, "github.com"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn test_netrc_parse_no_password() {
        let content = "machine github.com login foo";
        assert_eq!(netrc_parse(content, "github.com"), None);
    }

    #[test]
    fn test_netrc_parse_newlines() {
        let content = "machine github.com\nlogin foo\npassword bar";
        assert_eq!(
            netrc_parse(content, "github.com"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn test_netrc_parse_comments() {
        let content = "# my github login\nmachine github.com\nlogin foo\n# todo: keep this secret!\npassword bar";
        assert_eq!(
            netrc_parse(content, "github.com"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }
}

/// Normalizes a git remote URL into a forge's web/API base URL: `http(s)`
/// URLs simply have their path stripped, while other schemes (e.g. `ssh`)
/// are rewritten to `https://<host>` (dropping user info and port).
pub fn normalize_forge_url(remote_url: &Url) -> Result<Url, CommandError> {
    if matches!(remote_url.scheme(), "http" | "https") {
        let mut url = remote_url.clone();
        url.set_path("");
        Ok(url)
    } else {
        let host = remote_url
            .host_str()
            .ok_or_else(|| user_error(format!("Remote URL has no host: {remote_url}")))?;
        Url::parse(&format!("https://{host}"))
            .map_err(|e| user_error(format!("Failed to construct forge URL: {e}")))
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

#[cfg(test)]
mod nfu_tests {
    use super::*;

    #[test]
    fn test_normalize_forge_url_http() {
        let url = Url::parse("https://github.com/user/repo.git").unwrap();
        let normalized = normalize_forge_url(&url).unwrap();
        assert_eq!(normalized.as_str(), "https://github.com/");
    }

    #[test]
    fn test_normalize_forge_url_ssh() {
        let url = Url::parse("ssh://git@github.com/user/repo.git").unwrap();
        let normalized = normalize_forge_url(&url).unwrap();
        assert_eq!(normalized.as_str(), "https://github.com/");
    }
}
