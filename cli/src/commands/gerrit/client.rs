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

//! A client for the Gerrit REST API, combined with the repository metadata
//! needed to interact with a specific Gerrit project.

use std::sync::LazyLock;

use base64::Engine as _;
use regex::Regex;
use reqwest::Method;
use reqwest::Url;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde_json::Value;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::commands::cr::review::Check;
use crate::commands::cr::review::CheckState;
use crate::commands::cr::review::CodeReview;
use crate::commands::cr::review::CodeReviewState;
use crate::commands::cr::util;
use crate::ui::Ui;

static PROJECT_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/(a/)?(.*?)(\.git)?$").expect("valid regex"));

static NON_UPPER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[^A-Z]+").expect("valid regex"));

/// Client for the Gerrit REST API for a specific project.
///
/// - Loads credentials from `~/.netrc`.
/// - Adds an HTTP Basic Auth header to requests.
/// - Strips Gerrit's magic prefix from JSON responses.
/// - Returns an error on HTTP errors.
pub struct GerritClient {
    /// The URL of the Gerrit instance.
    pub forge_url: Url,
    pub project_id: String,
    #[allow(dead_code)]
    pub default_merge_target: String,
    /// `forge_url` with `/a/` appended, used as the base for authenticated
    /// API calls.
    api_url: Url,
    http: HttpClient,
}

impl GerritClient {
    pub async fn new(ui: &Ui, command: &CommandHelper, remote: &str) -> Result<Self, CommandError> {
        let workspace_command = command.workspace_helper(ui).await?;
        let settings = workspace_command.settings();
        let remote_url = util::get_remote_url(remote)?;

        let forge_url = if let Ok(review_url) = settings.get_string("gerrit.review-url") {
            Url::parse(&review_url)
                .map_err(|e| user_error(format!("Invalid gerrit.review-url: {e}")))?
        } else {
            util::normalize_forge_url(&remote_url)?
        };

        let project_id = parse_project_id(&remote_url)?;

        let default_merge_target =
            if let Ok(branch) = settings.get_string("gerrit.default-remote-branch") {
                branch
            } else {
                util::get_merge_target(remote)?
            };

        let host = forge_url
            .host_str()
            .ok_or_else(|| user_error(format!("Gerrit URL has no host: {forge_url}")))?;
        let (user, password) = util::netrc_read(host).ok_or_else(|| {
            user_error(format!("Could not find credentials for {host} in ~/.netrc"))
        })?;
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {credentials}"))
                .map_err(|e| user_error(format!("Invalid Gerrit credentials: {e}")))?,
        );
        let http = HttpClient::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| user_error(format!("Failed to build Gerrit HTTP client: {e}")))?;

        let mut api_url = forge_url.clone();
        api_url.set_path("/a/");

        Ok(Self {
            forge_url,
            project_id,
            default_merge_target,
            api_url,
            http,
        })
    }

    pub fn get(&self, path: &str) -> Result<Value, CommandError> {
        self.call(Method::GET, path, None)
    }

    pub fn _post(&self, path: &str, body: &Value) -> Result<Value, CommandError> {
        self.call(Method::POST, path, Some(body))
    }

    pub fn _put(&self, path: &str, body: &Value) -> Result<Value, CommandError> {
        self.call(Method::PUT, path, Some(body))
    }

    /// Sends a request to the Gerrit API, stripping the magic `)]}'` prefix
    /// from the response before parsing it as JSON.
    pub fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, CommandError> {
        let (status, cleaned) = self.send(method, path, body)?;

        if status.as_u16() == 401 {
            return Err(user_error(
                "Authentication failed. Check your ~/.netrc credentials.",
            ));
        }
        if !status.is_success() {
            return Err(user_error(format!(
                "Gerrit API request failed ({status}): {cleaned}"
            )));
        }

        serde_json::from_str(&cleaned)
            .map_err(|e| user_error(format!("Failed to parse Gerrit API response: {e}")))
    }

    /// Like [`Self::get`], but returns `Ok(None)` instead of an error if the
    /// request fails with an HTTP 404 (e.g. because a Gerrit plugin isn't
    /// installed on this instance).
    pub fn get_optional(&self, path: &str) -> Result<Option<Value>, CommandError> {
        let (status, cleaned) = self.send(Method::GET, path, None)?;

        if status.as_u16() == 404 {
            return Ok(None);
        }
        if status.as_u16() == 401 {
            return Err(user_error(
                "Authentication failed. Check your ~/.netrc credentials.",
            ));
        }
        if !status.is_success() {
            return Err(user_error(format!(
                "Gerrit API request failed ({status}): {cleaned}"
            )));
        }

        serde_json::from_str(&cleaned)
            .map(Some)
            .map_err(|e| user_error(format!("Failed to parse Gerrit API response: {e}")))
    }

    /// Sends a request to the Gerrit API and returns the response's status
    /// code along with its body, stripped of the magic `)]}'` prefix used to
    /// guard against XSSI attacks.
    fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(reqwest::StatusCode, String), CommandError> {
        let url = self
            .api_url
            .join(path)
            .map_err(|e| user_error(format!("Invalid Gerrit API path '{path}': {e}")))?;
        let mut request = self.http.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .map_err(|e| user_error(format!("Gerrit API request failed: {e}")))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|e| user_error(format!("Failed to read Gerrit API response: {e}")))?;
        let cleaned = text
            .trim_start_matches(|c| ")]}':\n".contains(c))
            .to_string();

        Ok((status, cleaned))
    }

    /// Parses a Gerrit change (as returned by the `changes/` REST endpoint
    /// with `o=SUBMIT_REQUIREMENTS`) into a forge-agnostic [`CodeReview`].
    pub fn parse_cr(&self, change: &Value) -> Result<CodeReview, CommandError> {
        let forge_url = &self.forge_url;
        let number = change["_number"]
            .as_u64()
            .ok_or_else(|| user_error("Gerrit change is missing '_number'"))?;

        let mut checks: Vec<Check> = self
            .get_checks(number)?
            .into_iter()
            .filter(|check| !matches!(check["state"].as_str(), Some("SUCCESSFUL" | "NOT_RELEVANT")))
            .map(|check| {
                let name = check["checker_name"]
                    .as_str()
                    .or_else(|| check["checker_uuid"].as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let state = match check["state"].as_str() {
                    Some("SUCCESSFUL") => CheckState::Pass,
                    Some("NOT_RELEVANT") => CheckState::Other,
                    Some("FAILED") => CheckState::Fail,
                    _ => CheckState::Other,
                };
                let url = check["url"].as_str().and_then(|s| Url::parse(s).ok());
                Check { name, url, state }
            })
            .collect();

        let submit_requirements = change
            .get("submit_requirements")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let blockers: Vec<Check> = submit_requirements
            .into_iter()
            .filter(|req| !matches!(req["status"].as_str(), Some("SATISFIED" | "NOT_APPLICABLE")))
            .map(|req| {
                let raw_name = req["name"].as_str().unwrap_or_default();
                let name = NON_UPPER_RE.replace_all(raw_name, "").into_owned();
                let state = match req["status"].as_str() {
                    Some("REJECTED") => CheckState::Fail,
                    Some("NEED" | "UNSATISFIED") => CheckState::InProgress,
                    Some("SATISFIED") => CheckState::Pass,
                    Some("NOT_APPLICABLE") => CheckState::Other,
                    _ => CheckState::Unknown,
                };
                Check {
                    name,
                    url: Some(forge_url.clone()),
                    state,
                }
            })
            .collect();

        let is_private = change
            .get("is_private")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let work_in_progress = change
            .get("work_in_progress")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let state = state_for(is_private, work_in_progress, !blockers.is_empty());

        checks.extend(blockers);

        let unresolved_comments = change
            .get("unresolved_comment_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let title = change["subject"].as_str().unwrap_or_default().to_string();
        let url = forge_url
            .join(&format!("/c/{number}"))
            .map_err(|e| user_error(format!("Invalid change URL: {e}")))?;

        Ok(CodeReview {
            id: format!("c{number}"),
            title,
            url,
            state_name: match state {
                CodeReviewState::Private => "Private",
                CodeReviewState::Draft => "Draft",
                CodeReviewState::Blocked => "Blocked",
                CodeReviewState::Accepted => "Accepted",
                _ => unreachable!("Gerrit review states are exhaustive"),
            }
            .to_string(),
            state,
            checks,
            unresolved_comments,
        })
    }

    /// Fetches CI check statuses for a change via the Gerrit checks plugin.
    ///
    /// Returns an empty list if the checks plugin isn't installed on this
    /// Gerrit instance.
    fn get_checks(&self, change_number: u64) -> Result<Vec<Value>, CommandError> {
        let path = format!("changes/{change_number}/revisions/current/checks?o=CHECKER");
        let checks = self
            .get_optional(&path)?
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        Ok(checks)
    }
}

fn parse_project_id(remote_url: &Url) -> Result<String, CommandError> {
    PROJECT_ID_RE
        .captures(remote_url.path())
        .map(|caps| caps[2].to_string())
        .ok_or_else(|| {
            user_error(format!(
                "Invalid Gerrit remote URL format: {remote_url}. Expected format: /project/path"
            ))
        })
}

/// Parses the change number from a Gerrit change identifier string (e.g.
/// "c123" or "123").
pub fn parse_cr_number(identifier: &str) -> Result<u64, CommandError> {
    let trimmed = identifier.trim();
    let number = trimmed.strip_prefix('c').unwrap_or(trimmed);
    number.parse().map_err(|e| {
        user_error(format!(
            "Invalid Gerrit change identifier '{identifier}': {e}"
        ))
    })
}

/// Parses the change number and optional revision from a Gerrit change
/// identifier string (e.g. "c123/4", "123/4", "c123", or "123").
///
/// Returns a tuple of (change_number, optional_revision).
pub fn parse_cr_number_with_revision(identifier: &str) -> Result<(u64, Option<u64>), CommandError> {
    let trimmed = identifier.trim();
    let without_prefix = trimmed.strip_prefix('c').unwrap_or(trimmed);

    if let Some((change_part, revision_part)) = without_prefix.split_once('/') {
        let change_number = change_part.parse().map_err(|e| {
            user_error(format!(
                "Invalid Gerrit change number in '{identifier}': {e}"
            ))
        })?;
        let revision = revision_part.parse().map_err(|e| {
            user_error(format!(
                "Invalid Gerrit revision number in '{identifier}': {e}"
            ))
        })?;
        Ok((change_number, Some(revision)))
    } else {
        let change_number = without_prefix.parse().map_err(|e| {
            user_error(format!(
                "Invalid Gerrit change identifier '{identifier}': {e}"
            ))
        })?;
        Ok((change_number, None))
    }
}

fn state_for(is_private: bool, work_in_progress: bool, blockers: bool) -> CodeReviewState {
    if is_private {
        CodeReviewState::Private
    } else if work_in_progress {
        CodeReviewState::Draft
    } else if blockers {
        CodeReviewState::Blocked
    } else {
        CodeReviewState::Accepted
    }
}
