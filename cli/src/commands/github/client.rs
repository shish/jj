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

//! A client for the GitHub REST/GraphQL API, combined with the repository
//! metadata needed to interact with a specific GitHub project.
//!
//! This merges the Python `GitHubClient` (`forges/github/lib/client.py`) and
//! `GitHubInfo` (`forges/github/lib/info.py`) into a single type, since there
//! is no need to keep them separate (or to share the `ForgeInfo` base class)
//! in the Rust port.

use std::env;
use std::sync::LazyLock;

use regex::Regex;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::USER_AGENT;
use serde_json::Value;
use serde_json::json;

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
    LazyLock::new(|| Regex::new(r"^/([^/]+?)/([^/]+?)(\.git)?$").expect("valid regex"));

/// Client for the GitHub REST/GraphQL API for a specific project.
pub struct GitHubClient {
    /// The URL of the GitHub instance's web frontend (e.g.
    /// `https://github.com`).
    pub forge_url: Url,
    pub default_merge_target: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    /// The base URL of the GitHub API (e.g. `https://api.github.com`).
    api_url: Url,
    http: HttpClient,
}

impl GitHubClient {
    pub async fn new(
        _ui: &Ui,
        _command: &CommandHelper,
        remote: &str,
    ) -> Result<Self, CommandError> {
        let remote_url = util::get_remote_url(remote)?;
        let forge_url = util::normalize_forge_url(&remote_url)?;

        let api_url = if let Some(forge_host) = forge_url.host_str() {
            if forge_host.ends_with("github.com") || forge_host.ends_with("ghe.com") {
                let mut u2 = forge_url.clone();
                u2.set_host(Some(format!("api.{forge_host}").as_str()))
                    .map_err(|e| user_error(format!("Invalid GitHub url: {e}")))?;
                u2
            } else {
                forge_url.clone()
            }
        } else {
            forge_url.clone()
        };

        let token = Self::resolve_token(&forge_url)?;
        // Wherever our token comes from, make sure it's set in the
        // environment variable for the `gh` CLI to use (we can remove
        // this if we completely replace `gh` and we're sure we won't
        // need it anymore).
        // `unsafe` safety check: happens before we start any threads
        unsafe {
            env::set_var("GITHUB_TOKEN", &token);
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| user_error(format!("Invalid GitHub token: {e}")))?,
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("jj-cr"));
        let http = HttpClient::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| user_error(format!("Failed to build GitHub HTTP client: {e}")))?;

        let (repo_owner, repo_name) = parse_project_id(&remote_url)?;

        let mut client = Self {
            forge_url,
            default_merge_target: String::new(),
            repo_id: String::new(),
            repo_owner,
            repo_name,
            api_url,
            http,
        };

        let repo_info = client.get_repo_info()?;
        client.repo_id = repo_info["id"].as_str().unwrap_or_default().to_string();
        //client.repo_owner = repo_info["owner"]["login"]
        //    .as_str()
        //    .unwrap_or_default()
        //    .to_string();
        //client.repo_name = repo_info["name"].as_str().unwrap_or_default().to_string();
        client.default_merge_target = match repo_info["defaultBranchRef"]["name"].as_str() {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => util::get_merge_target(remote)?,
        };

        Ok(client)
    }

    pub fn resolve_token(base_url: &Url) -> Result<String, CommandError> {
        for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
            if let Ok(token) = env::var(var)
                && !token.is_empty()
            {
                return Ok(token);
            }
        }

        let host = base_url.host_str().unwrap_or_default();
        if let Some((_, password)) = util::netrc_read(host) {
            return Ok(password);
        }

        if let Some(token) = read_gh_hosts(host) {
            return Ok(token);
        }

        Err(user_error(format!(
            "Could not find a GitHub token. Set the GITHUB_TOKEN or GH_TOKEN environment \
             variable, add credentials for {host} to ~/.netrc, or authenticate with the gh CLI \
             (`gh auth login`)"
        )))
    }

    /// Runs a GraphQL query against the GitHub API and returns the `data`
    /// field of the response.
    pub fn graphql(&self, query: &str, variables: Value) -> Result<Value, CommandError> {
        let mut body = serde_json::Map::new();
        body.insert("query".to_string(), Value::String(query.to_string()));
        if !variables.is_null() {
            body.insert("variables".to_string(), variables);
        }

        let url = self
            .api_url
            .join("/graphql")
            .map_err(|e| user_error(format!("Invalid GitHub API URL: {e}")))?;
        let response = self
            .http
            .post(url)
            .json(&Value::Object(body))
            .send()
            .map_err(|e| user_error(format!("GitHub API request failed: {e}")))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|e| user_error(format!("Failed to parse GitHub API response: {e}")))?;
        let js: Value = serde_json::from_str(&text)
            .map_err(|e| user_error(format!("Failed to parse GitHub API response: {e} {text}")))?;
        if !status.is_success() {
            return Err(user_error(format!(
                "GitHub API request failed ({status}): {js}"
            )));
        }
        if let Some(errors) = js.get("errors") {
            return Err(user_error(format!(
                "GraphQL query failed: {}",
                serde_json::to_string_pretty(errors).unwrap_or_default()
            )));
        }

        Ok(js["data"].clone())
    }

    /// Runs a REST POST request against the GitHub API.
    ///
    /// Returns `(status_code, response_json)` where `response_json` is `null`
    /// for responses with an empty body (e.g. `204 No Content`).
    pub fn rest_post(
        &self,
        path: &str,
        body: Option<Value>,
    ) -> Result<(StatusCode, Value), CommandError> {
        let url = self
            .api_url
            .join(path)
            .map_err(|e| user_error(format!("Invalid GitHub API URL: {e}")))?;

        let mut request = self
            .http
            .post(url)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10");
        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request
            .send()
            .map_err(|e| user_error(format!("GitHub API request failed: {e}")))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|e| user_error(format!("Failed to parse GitHub API response: {e}")))?;

        let parsed_json = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str::<Value>(&text).map_err(|e| {
                user_error(format!("Failed to parse GitHub API response: {e} {text}"))
            })?
        };

        if !status.is_success() {
            let body_text = if parsed_json.is_null() {
                text.trim().to_string()
            } else {
                parsed_json.to_string()
            };
            return Err(user_error(format!(
                "GitHub API request failed ({status}): {body_text}"
            )));
        }

        Ok((status, parsed_json))
    }

    fn get_repo_info(&self) -> Result<Value, CommandError> {
        let data = self.graphql(
            r#"
                fragment repo on Repository {
                    id
                    name
                    owner { login }
                    viewerPermission
                    defaultBranchRef { name }
                    isPrivate
                }
                query RepositoryNetwork($owner: String!, $name: String!) {
                    repository(owner: $owner, name: $name) {
                        ...repo
                        parent {
                            ...repo
                        }
                    }
                }
            "#,
            json!({
                "owner": self.repo_owner,
                "name": self.repo_name,
            }),
        )?;
        Ok(data["repository"].clone())
    }
}

/// Common fields when we query ci/cd status
pub const STATUS_CHECK_FIELDS: &str = r#"
    commits(last: 1) {
        nodes {
            commit {
                statusCheckRollup {
                    contexts(first: 100) {
                        nodes {
                            __typename
                            ... on StatusContext {
                                context
                                state
                                targetUrl
                            }
                            ... on CheckRun {
                                name
                                status
                                conclusion
                                detailsUrl
                            }
                        }
                    }
                }
            }
        }
    }
"#;

/// Common fields when we query review status
pub const REVIEW_FIELDS: &str = r#"
    reviews(first: 100) {
        nodes {
            state
        }
    }
"#;

/// Common fields when we query review status
pub const REVIEW_THREAD_FIELDS: &str = r#"
    reviewThreads(first: 100) {
        nodes {
            isResolved
        }
    }
"#;

/// Turns the nested `commits.nodes[0].commit.statusCheckRollup.contexts.nodes`
/// structure (queried via [`STATUS_CHECK_FIELDS`]) into a flat list of
/// checks, merging the `CheckRun` and `StatusContext` variants into a single
/// shape.
pub fn flatten_checks(pr: &Value) -> Vec<Check> {
    fn state_from(value: Option<&str>) -> CheckState {
        match value {
            Some("SUCCESS") => CheckState::Pass,
            Some("PENDING") => CheckState::InProgress,
            Some("FAILURE") => CheckState::Fail,
            _ => CheckState::Other,
        }
    }

    let contexts: &[Value] = pr
        .get("commits")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
        .and_then(|node| node.get("commit"))
        .and_then(|commit| commit.get("statusCheckRollup"))
        .filter(|rollup| !rollup.is_null())
        .and_then(|rollup| rollup.get("contexts"))
        .and_then(|contexts| contexts.get("nodes"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    contexts
        .iter()
        .filter_map(|context| match context["__typename"].as_str() {
            Some("CheckRun") => Some(Check {
                name: context["name"].as_str().unwrap_or_default().to_string(),
                state: state_from(context["conclusion"].as_str()),
                url: context["detailsUrl"]
                    .as_str()
                    .and_then(|s| Url::parse(s).ok()),
            }),
            Some("StatusContext") => Some(Check {
                name: context["context"].as_str().unwrap_or_default().to_string(),
                state: state_from(context["state"].as_str()),
                url: context["targetUrl"]
                    .as_str()
                    .and_then(|s| Url::parse(s).ok()),
            }),
            _ => None,
        })
        .collect()
}

/// Determines a PR's overall review state (e.g. "Draft", "Accepted",
/// "Rejected", "Needs Review") from various PR attributes.
pub fn pr_to_state(pr: &Value) -> CodeReviewState {
    let is_draft = pr["isDraft"].as_bool().unwrap_or(false);
    if is_draft {
        return CodeReviewState::Draft;
    }

    let reviews = pr["reviews"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let has_approved = reviews
        .iter()
        .any(|r| r["state"].as_str() == Some("APPROVED"));
    let has_rejected = reviews
        .iter()
        .any(|r| r["state"].as_str() == Some("CHANGES_REQUESTED"));

    if has_rejected {
        CodeReviewState::Rejected
    } else if has_approved {
        CodeReviewState::Accepted
    } else {
        CodeReviewState::NeedsReview
    }
}

/// Counts the number of unresolved review threads (queried via
/// [`REVIEW_THREAD_FIELDS`]).
pub fn count_unresolved(pr: &Value) -> i64 {
    pr.get("reviewThreads")
        .and_then(|threads| threads.get("nodes"))
        .and_then(Value::as_array)
        .map(|threads| {
            threads
                .iter()
                .filter(|thread| !thread["isResolved"].as_bool().unwrap_or(true))
                .count() as i64
        })
        .unwrap_or(0)
}

/// Parses a PullRequest GraphQL node (as returned by a query built with
/// [`STATUS_CHECK_FIELDS`], [`REVIEW_FIELDS`], and [`REVIEW_THREAD_FIELDS`])
/// into a forge-agnostic [`CodeReview`].
pub fn parse_cr(pr: &Value) -> Result<CodeReview, CommandError> {
    let number = pr["number"]
        .as_u64()
        .ok_or_else(|| user_error("GitHub PR is missing 'number'"))?;
    let url = pr["url"]
        .as_str()
        .ok_or_else(|| user_error("GitHub PR is missing 'url'"))?;
    let url = Url::parse(url).map_err(|e| user_error(format!("Invalid PR URL: {e}")))?;
    let state = pr_to_state(pr);

    Ok(CodeReview {
        id: format!("#{number}"),
        title: pr["title"].as_str().unwrap_or_default().to_string(),
        url,
        state_name: match state {
            CodeReviewState::Draft => "Draft",
            CodeReviewState::Rejected => "Rejected",
            CodeReviewState::Accepted => "Accepted",
            CodeReviewState::NeedsReview => "Needs Review",
            _ => unreachable!("GitHub review states are exhaustive"),
        }
        .to_string(),
        state,
        checks: flatten_checks(pr),
        unresolved_comments: count_unresolved(pr),
    })
}

/// Parse the project ID (user/repo) from a GitHub remote URL.
fn parse_project_id(remote_url: &Url) -> Result<(String, String), CommandError> {
    PROJECT_ID_RE
        .captures(remote_url.path())
        .map(|caps| (caps[1].to_string(), caps[2].to_string()))
        .ok_or_else(|| {
            user_error(format!(
                "Invalid GitHub remote URL format: {remote_url}. Expected format: owner/repo"
            ))
        })
}

/// Parses the PR number from a GitHub PR identifier string (e.g. "#123").
pub fn parse_pr_number(identifier: &str) -> Result<i64, CommandError> {
    identifier.trim_start_matches('#').parse().map_err(|e| {
        user_error(format!(
            "Invalid pull request identifier '{identifier}': {e}"
        ))
    })
}

/// `hosts.yml` appears to be consistently trivial, so let's parse it
/// manually rather than adding a dependency on a YAML parser or the `gh`
/// CLI.
fn read_gh_hosts(host: &str) -> Option<String> {
    let home = etcetera::home_dir().ok()?;
    let content =
        std::fs::read_to_string(home.join(".config").join("gh").join("hosts.yml")).ok()?;

    let mut current_host: Option<&str> = None;
    for line in content.lines() {
        // Top-level (unindented) keys are hostnames.
        if !line.is_empty() && !line.starts_with(char::is_whitespace) {
            current_host = Some(line.trim_end().trim_end_matches(':'));
        } else if current_host == Some(host)
            && let Some((key, value)) = line.trim().split_once(':')
            && key == "oauth_token"
        {
            let token = value.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}
