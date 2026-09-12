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

//! Helpers for integration-testing the Forgejo backend against the local
//! Forgejo instance from `compose.yml`.

use std::fs;
use std::time::Duration;

use reqwest::Method;
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde_json::Value;
use serde_json::json;

use super::TestEnvironment;
use super::TestWorkDir;

const FORGEJO_FORGE_URL: &str = "http://forgejo.localhost:8082";
/// The admin account created by the `post_start` hook in `compose.yml`.
const FORGEJO_USER: &str = "admin2";
const FORGEJO_PASSWORD: &str = "secret";

pub(crate) struct ForgejoTestRepo {
    env: TestEnvironment,
    forge_url: Url,
    api_url: Url,
    repo_name: String,
    token_name: String,
    token: String,
    http: Client,
    /// Whether dropping this instance should delete the remote repository and
    /// the API token. Only the instance that created them is responsible for
    /// cleaning them up.
    owns_remote: bool,
}

impl ForgejoTestRepo {
    pub(crate) fn maybe_new() -> Option<Self> {
        if !Self::is_available() {
            eprintln!(
                "Skipping Forgejo integration test: Forgejo test server is unavailable at \
                 {FORGEJO_FORGE_URL}"
            );
            return None;
        }

        Some(Self::new())
    }

    fn new() -> Self {
        let token_name = format!("jj-test-{:08x}", rand::random::<u32>());
        let token = Self::create_token(&token_name);
        let repo_name = format!("ztst-{:08x}", rand::random::<u32>());

        let mut repo = Self::create_env(repo_name, token_name, token, true);
        repo.create_remote_repo();
        repo.clone_repo();
        repo
    }

    pub(crate) fn fresh_clone(&self) -> Self {
        let repo = Self::create_env(
            self.repo_name.clone(),
            self.token_name.clone(),
            self.token.clone(),
            false,
        );
        repo.clone_repo();
        repo
    }

    /// Sets up an isolated test environment with Forgejo credentials in place,
    /// without touching the remote yet.
    fn create_env(repo_name: String, token_name: String, token: String, owns_remote: bool) -> Self {
        let env = TestEnvironment::default();
        let repo = Self {
            env,
            forge_url: Self::forge_url(),
            api_url: Self::api_url(),
            repo_name,
            token_name,
            http: Self::build_client(&token),
            token,
            owns_remote,
        };
        repo.write_netrc();
        repo
    }

    fn forge_url() -> Url {
        Url::parse(FORGEJO_FORGE_URL).expect("valid Forgejo URL")
    }

    fn api_url() -> Url {
        Self::forge_url()
            .join("/api/v1/")
            .expect("valid Forgejo API URL")
    }

    fn build_client(token: &str) -> Client {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("token {token}"))
                .expect("valid Forgejo authorization header"),
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        Client::builder()
            .default_headers(headers)
            .user_agent("jj-forgejo-integration-test")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build Forgejo API client")
    }

    fn is_available() -> bool {
        let Ok(http) = Client::builder().timeout(Duration::from_secs(5)).build() else {
            return false;
        };
        let Ok(url) = Self::api_url().join("version") else {
            return false;
        };
        http.get(url)
            .send()
            .is_ok_and(|response| response.status().is_success())
    }

    /// Creates an API token for the test admin user, authenticating with the
    /// password from `compose.yml`.
    fn create_token(token_name: &str) -> String {
        let token = Self::token_request(
            Method::POST,
            &format!("users/{FORGEJO_USER}/tokens"),
            Some(&json!({"name": token_name, "scopes": ["all"]})),
        )
        .expect("create Forgejo API token");
        token["sha1"]
            .as_str()
            .expect("Forgejo token response should include sha1")
            .to_string()
    }

    /// Sends a request to Forgejo's token endpoints, which only accept basic
    /// authentication.
    fn token_request(method: Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| err.to_string())?;
        let url = Self::api_url().join(path).map_err(|err| err.to_string())?;
        let mut request = http
            .request(method, url)
            .basic_auth(FORGEJO_USER, Some(FORGEJO_PASSWORD));
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().map_err(|err| err.to_string())?;
        let status = response.status();
        let text = response.text().map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("{status}: {text}"));
        }
        if text.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str(&text).map_err(|err| err.to_string())
        }
    }

    /// Forgejo accepts an API token as the password for both the REST API and
    /// git-over-HTTP.
    fn write_netrc(&self) {
        let host = self
            .forge_url
            .host_str()
            .expect("Forgejo URL should have a host");
        fs::write(
            self.env.home_dir().join(".netrc"),
            format!(
                "machine {host} login {FORGEJO_USER} password {}\n",
                self.token
            ),
        )
        .expect("write Forgejo credentials");
    }

    fn create_remote_repo(&mut self) {
        self.api_call(
            Method::POST,
            "user/repos",
            Some(&json!({
                "name": self.repo_name,
                "private": true,
                "auto_init": true,
                "default_branch": "main",
            })),
        );
    }

    fn clone_repo(&self) {
        let repo_url = self
            .forge_url
            .join(&format!("/{FORGEJO_USER}/{}.git", self.repo_name))
            .expect("construct Forgejo git URL")
            .to_string();
        let work_dir = self.clone_dir();
        work_dir
            .run_jj(["git", "clone", "--colocate", &repo_url, "."])
            .success();
    }

    fn api_call(&self, method: Method, path: &str, body: Option<&Value>) -> Value {
        self.api_call_result(method, path, body)
            .unwrap_or_else(|err| panic!("Forgejo API call failed for '{path}': {err}"))
    }

    fn api_call_result(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let url = self.api_url.join(path).map_err(|err| err.to_string())?;
        let mut request = self.http.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().map_err(|err| err.to_string())?;
        let status = response.status();
        let text = response.text().map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("{status}: {text}"));
        }
        if text.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str(&text).map_err(|err| err.to_string())
        }
    }

    fn clone_dir(&self) -> TestWorkDir<'_> {
        self.env.work_dir("").create_dir_all("clone");
        self.env.work_dir("clone")
    }

    pub(crate) fn run_jj(&self, args: &[&str]) -> String {
        self.clone_dir().run_jj(args).success().stdout.into_raw()
    }

    pub(crate) fn run_jj_json(&self, args: &[&str]) -> Value {
        let output = self.run_jj(args);
        serde_json::from_str(&output).unwrap_or_else(|err| {
            panic!("failed to parse jj output as JSON: {err}\noutput:\n{output}")
        })
    }

    pub(crate) fn write_file(&self, relative_path: &str, content: &str) {
        self.clone_dir().write_file(relative_path, content);
    }

    pub(crate) fn read_file(&self, relative_path: &str) -> String {
        self.clone_dir().read_file(relative_path).to_string()
    }

    pub(crate) fn file_exists(&self, relative_path: &str) -> bool {
        self.clone_dir().root().join(relative_path).exists()
    }
}

impl Drop for ForgejoTestRepo {
    fn drop(&mut self) {
        if !self.owns_remote {
            return;
        }

        let _unused = self.api_call_result(
            Method::DELETE,
            &format!("repos/{FORGEJO_USER}/{}", self.repo_name),
            None,
        );
        let _unused = Self::token_request(
            Method::DELETE,
            &format!("users/{FORGEJO_USER}/tokens/{}", self.token_name),
            None,
        );
    }
}
