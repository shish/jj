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

//! A client for the Phabricator Conduit API, combined with the repository
//! metadata needed to interact with a specific Phabricator project.
//!
//! This merges the Python `PhabricatorClient`
//! (`forges/phabricator/lib/client.py`) and `PhabricatorInfo`
//! (`forges/phabricator/lib/info.py`) into a single type, since there is no
//! need to keep them separate (or to share the `ForgeInfo` base class) in the
//! Rust port.

use std::path::Path;
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client as HttpClient;
use serde_json::Value;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::commands::cr::util;
use crate::ui::Ui;

/// Client for the Phabricator Conduit API for a specific project.
///
/// - Loads `api.token` from `~/.arcrc` for the given forge URL.
/// - Adds `api.token` to POST request data.
/// - Returns an error on HTTP errors, or on Conduit-level errors.
pub struct PhabricatorClient {
    /// The URL of the Phabricator instance.
    pub forge_url: Url,
    /// The repository callsign of the project.
    pub project_id: String,
    pub default_merge_target: String,
    /// `forge_url` with `/api/` appended.
    api_url: Url,
    token: String,
    http: HttpClient,
}

impl PhabricatorClient {
    pub async fn new(
        _ui: &Ui,
        command: &CommandHelper,
        remote: &str,
    ) -> Result<Self, CommandError> {
        let remote_url = util::get_remote_url(remote)?;
        let repo_config = read_arcconfig(command.workspace_loader()?.workspace_root());

        let forge_url = match repo_config.get("phabricator.uri").and_then(Value::as_str) {
            Some(uri) => {
                Url::parse(uri).map_err(|e| user_error(format!("Invalid phabricator.uri: {e}")))?
            }
            None => {
                let mut url = remote_url.clone();
                url.set_path("");
                url
            }
        };

        let api_url = forge_url
            .join("/api/")
            .map_err(|e| user_error(format!("Invalid Phabricator URL: {e}")))?;

        let host = forge_url
            .host_str()
            .ok_or_else(|| user_error(format!("Phabricator URL has no host: {forge_url}")))?;
        let token = read_arcrc_token(host)
            .ok_or_else(|| user_error(format!("API token for {host} not found in ~/.arcrc")))?;

        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| user_error(format!("Failed to build Phabricator HTTP client: {e}")))?;

        // Create an API client with blank project, then use the client
        // to search for a project with the given repo URI
        let mut client = Self {
            forge_url,
            project_id: String::new(),
            default_merge_target: String::new(),
            api_url,
            token,
            http,
        };

        client.project_id = match repo_config
            .get("repository.callsign")
            .and_then(Value::as_str)
        {
            Some(callsign) => callsign.to_string(),
            None => {
                let result = client.call(
                    "diffusion.repository.search",
                    serde_json::json!({
                        "constraints": { "uris": [remote_url.to_string()] },
                    }),
                )?;
                let repos = result
                    .get("data")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let first = repos.first().ok_or_else(|| {
                    user_error(format!(
                        "Could not find a Phabricator repository for {remote_url}"
                    ))
                })?;
                first["fields"]["callsign"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            }
        };

        client.default_merge_target = match repo_config
            .get("arc.land.onto.default")
            .and_then(Value::as_str)
        {
            Some(target) => target.to_string(),
            None => util::get_merge_target(remote)?,
        };

        Ok(client)
    }

    /// Calls a Conduit API method, returning the `result` field of the
    /// response.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, CommandError> {
        let mut params = match params {
            Value::Object(map) => map,
            Value::Null => serde_json::Map::new(),
            _ => return Err(user_error("Phabricator call params must be an object")),
        };
        let mut conduit = serde_json::Map::new();
        conduit.insert("token".to_string(), Value::String(self.token.clone()));
        params.insert("__conduit__".to_string(), Value::Object(conduit));

        let url = self
            .api_url
            .join(method)
            .map_err(|e| user_error(format!("Invalid Phabricator API method '{method}': {e}")))?;
        let params_json = serde_json::to_string(&Value::Object(params))
            .map_err(|e| user_error(format!("Failed to encode Phabricator API params: {e}")))?;

        let response = self
            .http
            .post(url)
            .form(&[
                ("params", params_json.as_str()),
                ("output", "json"),
                ("__conduit__", "true"),
            ])
            .send()
            .map_err(|e| user_error(format!("Phabricator API request failed: {e}")))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|e| user_error(format!("Failed to read Phabricator API response: {e}")))?;
        if !status.is_success() {
            return Err(user_error(format!(
                "Phabricator API request failed ({status}): {text}"
            )));
        }

        let js: Value = serde_json::from_str(&text)
            .map_err(|e| user_error(format!("Failed to parse Phabricator API response: {e}")))?;
        if let Some(code) = js.get("error_code").filter(|c| !c.is_null()) {
            let info = js
                .get("error_info")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(user_error(format!(
                "Phabricator API error: {code} - {info}"
            )));
        }

        Ok(js["result"].clone())
    }
}

/// Parses the revision number from a Phabricator revision identifier string
/// (e.g. "D123" or "123").
pub fn parse_rev_number(identifier: &str) -> Result<u64, CommandError> {
    let trimmed = identifier.trim();
    let number = trimmed
        .strip_prefix('D')
        .or_else(|| trimmed.strip_prefix('d'))
        .unwrap_or(trimmed);
    number.parse().map_err(|e| {
        user_error(format!(
            "Invalid Phabricator revision identifier '{identifier}': {e}"
        ))
    })
}

/// Reads `.arcconfig` from the current directory, if present, returning an
/// empty object otherwise.
fn read_arcconfig(workspace_root: &Path) -> Value {
    std::fs::read_to_string(workspace_root.join(".arcconfig"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

/// Reads the API token for `host` out of `~/.arcrc`.
fn read_arcrc_token(host: &str) -> Option<String> {
    let home = etcetera::home_dir().ok()?;
    let content = std::fs::read_to_string(home.join(".arcrc")).ok()?;
    let data: Value = serde_json::from_str(&content).ok()?;
    let hosts = data.get("hosts")?.as_object()?;
    for (url, config) in hosts {
        if Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .as_deref()
            == Some(host)
        {
            return config
                .get("token")
                .and_then(Value::as_str)
                .map(String::from);
        }
    }
    None
}
