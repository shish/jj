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

//! A client for the Forgejo REST API, combined with the repository metadata
//! needed to interact with a specific Forgejo project.
//!
//! Forgejo's API is Gitea-compatible, so this should also work against Gitea
//! instances.

use std::collections::BTreeSet;
use std::env;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::USER_AGENT;
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
    LazyLock::new(|| Regex::new(r"^/([^/]+?)/([^/]+?)(\.git)?$").expect("valid regex"));

/// How many items to request per page from paginated API endpoints.
const PAGE_SIZE: usize = 50;

/// Forgejo has no API flag for draft pull requests; instead, a pull request is
/// a draft if its title starts with one of these prefixes (matching Forgejo's
/// default `repository.pull-request.WORK_IN_PROGRESS_PREFIXES`).
const WIP_PREFIXES: [&str; 2] = ["WIP:", "[WIP]"];

/// Client for the Forgejo REST API for a specific project.
pub struct ForgejoClient {
    /// The URL of the Forgejo instance's web frontend (e.g.
    /// `https://codeberg.org`).
    pub forge_url: Url,
    pub default_merge_target: String,
    pub repo_owner: String,
    pub repo_name: String,
    /// `forge_url` with `/api/v1/` appended.
    api_url: Url,
    http: HttpClient,
}

impl ForgejoClient {
    pub async fn new(
        _ui: &Ui,
        _command: &CommandHelper,
        remote: &str,
    ) -> Result<Self, CommandError> {
        let remote_url = util::get_remote_url(remote)?;
        let forge_url = util::normalize_forge_url(&remote_url)?;
        let api_url = forge_url
            .join("/api/v1/")
            .map_err(|e| user_error(format!("Invalid Forgejo URL: {e}")))?;

        let token = Self::resolve_token(&forge_url)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("token {token}"))
                .map_err(|e| user_error(format!("Invalid Forgejo token: {e}")))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static("jj-cr"));
        let http = HttpClient::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| user_error(format!("Failed to build Forgejo HTTP client: {e}")))?;

        let (repo_owner, repo_name) = parse_project_id(&remote_url)?;

        let mut client = Self {
            forge_url,
            default_merge_target: String::new(),
            repo_owner,
            repo_name,
            api_url,
            http,
        };

        let repo_info = client.get(&client.repo_path(""), &[])?;
        client.default_merge_target = match repo_info["default_branch"].as_str() {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => util::get_merge_target(remote)?,
        };

        Ok(client)
    }

    pub(crate) fn resolve_token(base_url: &Url) -> Result<String, CommandError> {
        for var in ["FORGEJO_TOKEN", "GITEA_TOKEN"] {
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

        Err(user_error(format!(
            "Could not find a Forgejo token. Set the FORGEJO_TOKEN or GITEA_TOKEN environment \
             variable, or add an access token as the password for {host} to ~/.netrc"
        )))
    }

    /// The API path of the current repository, with `suffix` appended (e.g.
    /// `repos/owner/name/pulls`).
    pub fn repo_path(&self, suffix: &str) -> String {
        let path = format!("repos/{}/{}", self.repo_owner, self.repo_name);
        if suffix.is_empty() {
            path
        } else {
            format!("{path}/{suffix}")
        }
    }

    /// Runs a GET request, returning the parsed response body.
    pub fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, CommandError> {
        let (_, body) = self.request(Method::GET, path, query, None)?;
        Ok(body)
    }

    /// Runs a GET request, returning `None` if the resource does not exist.
    pub fn get_optional(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Option<Value>, CommandError> {
        let (status, body) = self.send(Method::GET, path, query, None)?;
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        check_status(status, &body)?;
        Ok(Some(body))
    }

    /// Runs GET requests against a paginated endpoint until all items have
    /// been collected.
    pub fn get_all(&self, path: &str, query: &[(&str, &str)]) -> Result<Vec<Value>, CommandError> {
        let limit = PAGE_SIZE.to_string();
        let mut items = Vec::new();
        for page in 1.. {
            let page = page.to_string();
            let mut query = query.to_vec();
            query.push(("page", &page));
            query.push(("limit", &limit));

            let body = self.get(path, &query)?;
            let Value::Array(page_items) = body else {
                return Err(user_error(format!(
                    "Expected a list from the Forgejo API endpoint '{path}'"
                )));
            };
            let page_len = page_items.len();
            items.extend(page_items);
            if page_len < PAGE_SIZE {
                break;
            }
        }
        Ok(items)
    }

    /// Runs a POST request, returning the parsed response body.
    pub fn post(&self, path: &str, body: Value) -> Result<Value, CommandError> {
        let (_, body) = self.request(Method::POST, path, &[], Some(body))?;
        Ok(body)
    }

    /// Runs a PATCH request, returning the parsed response body.
    pub fn patch(&self, path: &str, body: Value) -> Result<Value, CommandError> {
        let (_, body) = self.request(Method::PATCH, path, &[], Some(body))?;
        Ok(body)
    }

    /// Runs a request against the Forgejo API, returning an error for
    /// non-success responses.
    fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<(StatusCode, Value), CommandError> {
        let (status, body) = self.send(method, path, query, body)?;
        check_status(status, &body)?;
        Ok((status, body))
    }

    /// Sends a request to the Forgejo API.
    ///
    /// Returns `(status_code, response_json)` where `response_json` is `null`
    /// for responses with an empty body (e.g. `204 No Content`). HTTP error
    /// statuses are returned as-is, rather than as an error.
    fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<(StatusCode, Value), CommandError> {
        let mut url = self
            .api_url
            .join(path)
            .map_err(|e| user_error(format!("Invalid Forgejo API path '{path}': {e}")))?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }

        let mut request = self.http.request(method, url);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .map_err(|e| user_error(format!("Forgejo API request failed: {e}")))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|e| user_error(format!("Failed to read Forgejo API response: {e}")))?;
        let parsed = if text.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str::<Value>(&text) {
                Ok(parsed) => parsed,
                // A non-JSON error body is most likely an error page from
                // something in front of Forgejo; keep it for the error
                // message instead of complaining about invalid JSON.
                Err(_) if !status.is_success() => Value::String(text.trim().to_string()),
                Err(e) => {
                    return Err(user_error(format!(
                        "Failed to parse Forgejo API response: {e} {text}"
                    )));
                }
            }
        };

        Ok((status, parsed))
    }

    /// The login name of the authenticated user.
    pub fn current_user_login(&self) -> Result<String, CommandError> {
        let user = self.get("user", &[])?;
        user["login"]
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| user_error("Forgejo /user response is missing 'login'"))
    }

    /// Lists pull requests, e.g. with `[("state", "open")]`.
    pub fn pull_requests(&self, query: &[(&str, &str)]) -> Result<Vec<Value>, CommandError> {
        self.get_all(&self.repo_path("pulls"), query)
    }

    /// Fetches a single pull request, returning `None` if it doesn't exist.
    pub fn pull_request(&self, number: i64) -> Result<Option<Value>, CommandError> {
        self.get_optional(&self.repo_path(&format!("pulls/{number}")), &[])
    }

    /// Finds the open pull request whose head branch is `head_ref_name`.
    ///
    /// The head filter is applied by the server, but we double-check it here
    /// so that an instance which ignores the parameter doesn't make us report
    /// some unrelated pull request.
    pub fn open_pull_request_by_head(
        &self,
        head_ref_name: &str,
    ) -> Result<Option<Value>, CommandError> {
        let pull_requests = self.pull_requests(&[("state", "open"), ("head", head_ref_name)])?;
        Ok(pull_requests
            .into_iter()
            .find(|pr| pr["head"]["ref"].as_str() == Some(head_ref_name)))
    }

    /// Fetches the reviews of a pull request.
    pub fn pull_reviews(&self, number: i64) -> Result<Vec<Value>, CommandError> {
        self.get_all(&self.repo_path(&format!("pulls/{number}/reviews")), &[])
    }

    /// Turns a pull request into a forge-agnostic [`CodeReview`], fetching the
    /// extra review and CI data that Forgejo doesn't include in the pull
    /// request itself.
    pub fn fetch_cr(&self, pr: &Value) -> Result<CodeReview, CommandError> {
        let number = pr["number"]
            .as_i64()
            .ok_or_else(|| user_error("Forgejo pull request is missing 'number'"))?;
        let url = pr["html_url"]
            .as_str()
            .ok_or_else(|| user_error("Forgejo pull request is missing 'html_url'"))?;
        let url = Url::parse(url)
            .map_err(|e| user_error(format!("Invalid pull request URL '{url}': {e}")))?;

        let reviews = self.pull_reviews(number)?;
        let state = pr_to_state(pr, &reviews);
        let checks = match pr["head"]["sha"].as_str() {
            Some(sha) if !sha.is_empty() => self.get_checks(sha)?,
            _ => vec![],
        };

        Ok(CodeReview {
            id: format!("#{number}"),
            title: pr["title"].as_str().unwrap_or_default().to_string(),
            url,
            state_name: state_name(state).to_string(),
            state,
            checks,
            unresolved_comments: self.count_unresolved(number, &reviews)?,
        })
    }

    /// Collects the latest commit status for each CI context of `sha`.
    fn get_checks(&self, sha: &str) -> Result<Vec<Check>, CommandError> {
        let combined = self.get(&self.repo_path(&format!("commits/{sha}/status")), &[])?;
        Ok(checks_from_combined_status(&combined))
    }

    /// Counts the review conversations of a pull request that nobody has
    /// marked as resolved.
    ///
    /// Forgejo resolves whole conversations at once, but only exposes
    /// individual comments, so comments are grouped by the line they are
    /// attached to.
    fn count_unresolved(&self, number: i64, reviews: &[Value]) -> Result<i64, CommandError> {
        let mut conversations = BTreeSet::new();
        for review in reviews {
            if review["comments_count"].as_i64().unwrap_or(0) == 0 {
                continue;
            }
            let Some(review_id) = review["id"].as_i64() else {
                continue;
            };
            let comments = self.get(
                &self.repo_path(&format!("pulls/{number}/reviews/{review_id}/comments")),
                &[],
            )?;
            for comment in comments.as_array().into_iter().flatten() {
                if !comment["resolver"].is_null() {
                    continue;
                }
                conversations.insert((
                    comment["path"].as_str().unwrap_or_default().to_string(),
                    comment["original_position"]
                        .as_i64()
                        .or_else(|| comment["position"].as_i64())
                        .unwrap_or_default(),
                ));
            }
        }
        Ok(conversations.len() as i64)
    }
}

/// Turns a non-success HTTP status into an error, using the `message` that
/// Forgejo reports errors with.
fn check_status(status: StatusCode, body: &Value) -> Result<(), CommandError> {
    if status.is_success() {
        return Ok(());
    }
    let message = body
        .get("message")
        .or(Some(body))
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .unwrap_or("no details");
    Err(user_error(format!(
        "Forgejo API request failed ({status}): {message}"
    )))
}

/// Turns the `statuses` of a combined commit status into a flat list of
/// checks.
pub fn checks_from_combined_status(combined: &Value) -> Vec<Check> {
    combined["statuses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|status| Check {
            name: match status["context"].as_str() {
                Some(context) if !context.is_empty() => context.to_string(),
                _ => status["description"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
            },
            state: match status["status"].as_str() {
                Some("success") => CheckState::Pass,
                Some("pending") => CheckState::InProgress,
                Some("failure" | "error") => CheckState::Fail,
                _ => CheckState::Other,
            },
            url: status["target_url"]
                .as_str()
                .filter(|url| !url.is_empty())
                .and_then(|url| Url::parse(url).ok()),
        })
        .collect()
}

/// Determines a pull request's overall review state from the pull request
/// itself and its reviews.
pub fn pr_to_state(pr: &Value, reviews: &[Value]) -> CodeReviewState {
    if pr["merged"].as_bool().unwrap_or(false) || pr["state"].as_str() == Some("closed") {
        return CodeReviewState::Closed;
    }
    if pr["draft"].as_bool().unwrap_or(false) || is_wip_title(pr["title"].as_str().unwrap_or("")) {
        return CodeReviewState::Draft;
    }

    // Dismissed reviews no longer count, and stale ones were made against an
    // older version of the pull request.
    let states = reviews
        .iter()
        .filter(|review| !review["dismissed"].as_bool().unwrap_or(false))
        .filter_map(|review| review["state"].as_str())
        .collect::<Vec<_>>();

    if states.contains(&"REQUEST_CHANGES") {
        CodeReviewState::Rejected
    } else if states.contains(&"APPROVED") {
        CodeReviewState::Accepted
    } else {
        CodeReviewState::NeedsReview
    }
}

fn state_name(state: CodeReviewState) -> &'static str {
    match state {
        CodeReviewState::Draft => "Draft",
        CodeReviewState::Rejected => "Rejected",
        CodeReviewState::Accepted => "Accepted",
        CodeReviewState::NeedsReview => "Needs Review",
        CodeReviewState::Closed => "Closed",
        _ => "Other",
    }
}

/// Whether a pull request title marks it as a work in progress (Forgejo's
/// equivalent of a draft).
pub fn is_wip_title(title: &str) -> bool {
    let title = title.to_ascii_uppercase();
    WIP_PREFIXES.iter().any(|prefix| title.starts_with(*prefix))
}

/// Adds a work-in-progress prefix to a pull request title, if it doesn't have
/// one already.
pub fn wip_title(title: &str) -> String {
    if is_wip_title(title) {
        title.to_string()
    } else {
        format!("{} {title}", WIP_PREFIXES[0])
    }
}

/// Parse the project ID (user/repo) from a Forgejo remote URL.
fn parse_project_id(remote_url: &Url) -> Result<(String, String), CommandError> {
    PROJECT_ID_RE
        .captures(remote_url.path())
        .map(|caps| (caps[1].to_string(), caps[2].to_string()))
        .ok_or_else(|| {
            user_error(format!(
                "Invalid Forgejo remote URL format: {remote_url}. Expected format: owner/repo"
            ))
        })
}

/// Parses the pull request number from an identifier string (e.g. "#123").
pub fn parse_pr_number(identifier: &str) -> Result<i64, CommandError> {
    identifier
        .trim()
        .trim_start_matches('#')
        .parse()
        .map_err(|e| {
            user_error(format!(
                "Invalid pull request identifier '{identifier}': {e}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_project_id() {
        let url = Url::parse("https://codeberg.org/user/repo.git").unwrap();
        assert_eq!(
            parse_project_id(&url).unwrap(),
            ("user".to_string(), "repo".to_string())
        );
        let url = Url::parse("ssh://git@codeberg.org/user/repo").unwrap();
        assert_eq!(
            parse_project_id(&url).unwrap(),
            ("user".to_string(), "repo".to_string())
        );
        let url = Url::parse("https://codeberg.org/repo").unwrap();
        assert!(parse_project_id(&url).is_err());
    }

    #[test]
    fn test_parse_pr_number() {
        assert_eq!(parse_pr_number("#12").unwrap(), 12);
        assert_eq!(parse_pr_number("12").unwrap(), 12);
        assert!(parse_pr_number("PR-12").is_err());
    }

    #[test]
    fn test_wip_titles() {
        assert!(is_wip_title("WIP: not done yet"));
        assert!(is_wip_title("[wip] not done yet"));
        assert!(!is_wip_title("Done"));
        assert_eq!(wip_title("Done"), "WIP: Done");
        assert_eq!(wip_title("WIP: Done"), "WIP: Done");
    }

    #[test]
    fn test_pr_to_state() {
        let open = json!({"title": "Fix it", "state": "open"});
        assert_eq!(pr_to_state(&open, &[]), CodeReviewState::NeedsReview);

        let draft = json!({"title": "Fix it", "state": "open", "draft": true});
        assert_eq!(pr_to_state(&draft, &[]), CodeReviewState::Draft);

        let wip = json!({"title": "WIP: Fix it", "state": "open"});
        assert_eq!(pr_to_state(&wip, &[]), CodeReviewState::Draft);

        let closed = json!({"title": "Fix it", "state": "closed"});
        assert_eq!(pr_to_state(&closed, &[]), CodeReviewState::Closed);

        let approved = [json!({"state": "APPROVED"}), json!({"state": "COMMENT"})];
        assert_eq!(pr_to_state(&open, &approved), CodeReviewState::Accepted);

        let rejected = [
            json!({"state": "APPROVED"}),
            json!({"state": "REQUEST_CHANGES"}),
        ];
        assert_eq!(pr_to_state(&open, &rejected), CodeReviewState::Rejected);

        let dismissed = [json!({"state": "REQUEST_CHANGES", "dismissed": true})];
        assert_eq!(pr_to_state(&open, &dismissed), CodeReviewState::NeedsReview);
    }

    #[test]
    fn test_checks_from_combined_status() {
        let combined = json!({
            "statuses": [
                {
                    "context": "build",
                    "status": "success",
                    "target_url": "https://codeberg.org/user/repo/actions/runs/1",
                },
                {"context": "test", "status": "pending", "target_url": ""},
                {"context": "lint", "status": "failure"},
                {"context": "", "description": "deploy", "status": "skipped"},
            ]
        });
        let checks = checks_from_combined_status(&combined);
        let states: Vec<_> = checks
            .iter()
            .map(|check| (check.name.as_str(), check.state))
            .collect();
        assert_eq!(
            states,
            vec![
                ("build", CheckState::Pass),
                ("test", CheckState::InProgress),
                ("lint", CheckState::Fail),
                ("deploy", CheckState::Other),
            ]
        );
        assert_eq!(
            checks[0].url.as_ref().map(Url::as_str),
            Some("https://codeberg.org/user/repo/actions/runs/1")
        );
        assert_eq!(checks[1].url, None);
    }

    #[test]
    fn test_checks_from_combined_status_empty() {
        assert!(checks_from_combined_status(&json!({})).is_empty());
        assert!(checks_from_combined_status(&json!({"statuses": null})).is_empty());
    }
}
