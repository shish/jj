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

use std::fs;

use base64::Engine as _;
use reqwest::Method;
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde_json::Value;
use serde_json::json;

use super::TestEnvironment;
use super::TestWorkDir;

const GERRIT_FORGE_URL: &str = "http://gerrit.localhost:8080";
const GERRIT_USER: &str = "admin";
const GERRIT_PASSWORD: &str = "secret";

pub(crate) struct GerritTestRepo {
    env: TestEnvironment,
    http: Client,
    forge_url: Url,
    repo_name: String,
    repo_url: String,
    /// Whether dropping this instance should delete the remote project. Only
    /// the instance that created it is responsible for cleaning it up.
    owns_remote: bool,
}

impl GerritTestRepo {
    pub(crate) fn maybe_new() -> Option<Self> {
        if !Self::is_available() {
            eprintln!(
                "Skipping Gerrit integration test: Gerrit test server is unavailable at \
                 {GERRIT_FORGE_URL}"
            );
            return None;
        }

        Some(Self::new())
    }

    fn new() -> Self {
        let repo_name = format!("ztst-{:04x}", rand::random::<u16>());
        let repo = Self::create_env(Self::build_client(), Self::forge_url(), repo_name, true);
        repo.create_remote_project();
        repo.clone_repo();
        repo
    }

    pub(crate) fn fresh_clone(&self) -> Self {
        let repo = Self::create_env(
            self.http.clone(),
            self.forge_url.clone(),
            self.repo_name.clone(),
            false,
        );
        repo.clone_repo();
        repo
    }

    /// Sets up an isolated test environment with Gerrit credentials in place,
    /// without touching the remote yet.
    fn create_env(http: Client, forge_url: Url, repo_name: String, owns_remote: bool) -> Self {
        let repo_url = forge_url
            .join(&format!("/{repo_name}.git"))
            .expect("construct Gerrit git URL")
            .to_string();

        let env = TestEnvironment::default();
        let repo = Self {
            env,
            http,
            forge_url,
            repo_name,
            repo_url,
            owns_remote,
        };
        repo.write_netrc();
        repo
    }

    fn forge_url() -> Url {
        Url::parse(GERRIT_FORGE_URL).expect("valid Gerrit URL")
    }

    fn build_client() -> Client {
        let mut headers = HeaderMap::new();
        let credentials = base64::engine::general_purpose::STANDARD
            .encode(format!("{GERRIT_USER}:{GERRIT_PASSWORD}"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {credentials}"))
                .expect("valid authorization header"),
        );
        Client::builder()
            .default_headers(headers)
            .build()
            .expect("build Gerrit API client")
    }

    fn is_available() -> bool {
        let Ok(forge_url) = Url::parse(GERRIT_FORGE_URL) else {
            return false;
        };
        let mut api_url = forge_url;
        api_url.set_path("/a/");
        Self::request(
            &Self::build_client(),
            &api_url,
            Method::GET,
            "accounts/self",
            None,
        )
        .is_ok()
    }

    fn write_netrc(&self) {
        fs::write(
            self.env.home_dir().join(".netrc"),
            format!("machine gerrit.localhost login {GERRIT_USER} password {GERRIT_PASSWORD}\n"),
        )
        .expect("write .netrc");
    }

    /// Creates the project on the Gerrit server and allows registered users to
    /// push to it.
    fn create_remote_project(&self) {
        self.api_call(
            Method::PUT,
            &format!("projects/{}", self.repo_name),
            Some(&json!({ "create_empty_commit": true })),
        );
        self.api_call(
            Method::POST,
            &format!("projects/{}/access", self.repo_name),
            Some(&json!({
                "add": {
                    "refs/heads/*": {
                        "permissions": {
                            "push": {
                                "rules": {
                                    "global:Registered-Users": {
                                        "action": "ALLOW",
                                        "force": true
                                    }
                                }
                            }
                        }
                    }
                }
            })),
        );
    }

    fn clone_repo(&self) {
        let clone_dir = self.clone_dir();
        clone_dir
            .run_jj(["git", "clone", "--colocate", &self.repo_url, "."])
            .success();
    }

    fn api_url(&self) -> Url {
        let mut api_url = self.forge_url.clone();
        api_url.set_path("/a/");
        api_url
    }

    fn api_call(&self, method: Method, path: &str, body: Option<&Value>) -> Value {
        self.api_call_result(method, path, body)
            .unwrap_or_else(|err| panic!("Gerrit API call failed for '{path}': {err}"))
    }

    fn api_call_result(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        Self::request(&self.http, &self.api_url(), method, path, body)
    }

    fn request(
        http: &Client,
        api_url: &Url,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let url = api_url
            .join(path)
            .map_err(|err| format!("invalid path '{path}': {err}"))?;
        let mut request = http.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .map_err(|err| format!("request failed: {err}"))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|err| format!("read response failed: {err}"))?;
        let cleaned = text
            .trim_start_matches(|c| ")]}'\n:".contains(c))
            .to_string();
        if !status.is_success() {
            return Err(format!("status {status}: {cleaned}"));
        }
        serde_json::from_str(&cleaned).map_err(|err| format!("invalid JSON: {err}"))
    }

    fn clone_dir(&self) -> TestWorkDir<'_> {
        self.env.work_dir("").create_dir_all("clone");
        self.env.work_dir("clone")
    }

    pub(crate) fn run_jj(&self, args: &[&str]) -> String {
        self.clone_dir().run_jj(args).success().stdout.into_raw()
    }

    pub(crate) fn run_jj_json(&self, args: &[&str]) -> Value {
        let text = self.run_jj(args);
        serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("failed to parse JSON output from jj: {err}\n{text}"))
    }

    pub(crate) fn write_file(&self, relative_path: &str, content: &str) {
        self.clone_dir().write_file(relative_path, content);
    }

    pub(crate) fn file_contents(&self, relative_path: &str) -> String {
        self.clone_dir().read_file(relative_path).to_string()
    }

    pub(crate) fn file_exists(&self, relative_path: &str) -> bool {
        self.clone_dir().root().join(relative_path).exists()
    }
}

impl Drop for GerritTestRepo {
    fn drop(&mut self) {
        if !self.owns_remote {
            return;
        }

        let open_changes = self
            .api_call_result(
                Method::GET,
                &format!("changes?q=status:open+project:{}", self.repo_name),
                None,
            )
            .ok();

        if let Some(Value::Array(changes)) = open_changes {
            for change in changes {
                if let Some(change_id) = change.get("id").and_then(Value::as_str) {
                    let _unused =
                        self.api_call_result(Method::DELETE, &format!("changes/{change_id}"), None);
                }
            }
        }

        let _unused = self.api_call_result(
            Method::DELETE,
            &format!("projects/{}", self.repo_name),
            None,
        );
    }
}
