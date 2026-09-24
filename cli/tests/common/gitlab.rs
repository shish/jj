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

//! Helpers for integration-testing the GitLab backend against the local
//! GitLab instance from `compose.yml`.

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

const GITLAB_FORGE_URL: &str = "http://gitlab.localhost:8083";
const GITLAB_TOKEN: &str = "secret";

pub struct GitlabTestRepo {
    env: TestEnvironment,
    forge_url: Url,
    api_url: Url,
    repo_name: String,
    group_id: Option<u32>,
    project_id: Option<u32>,
    http: Client,
    /// Whether dropping this instance should delete the remote repository and
    /// the group. Only the instance that created them is responsible for
    /// cleaning them up.
    owns_remote: bool,
}

impl GitlabTestRepo {
    pub(crate) fn maybe_new() -> Option<Self> {
        if !Self::is_available() {
            eprintln!(
                "Skipping GitLab integration test: GitLab test server is unavailable at \
                 {GITLAB_FORGE_URL}"
            );
            return None;
        }

        Some(Self::new())
    }

    fn new() -> Self {
        let repo_name = format!("ztst-{:08x}", rand::random::<u32>());

        let mut repo = Self::create_env(repo_name, None, None, true);
        repo.create_remote_repo();
        repo.clone_repo();
        repo
    }

    pub(crate) fn fresh_clone(&self) -> Self {
        let repo = Self::create_env(
            self.repo_name.clone(),
            self.group_id,
            self.project_id,
            false,
        );
        repo.clone_repo();
        repo
    }

    /// Sets up an isolated test environment with GitLab credentials in place,
    /// without touching the remote yet.
    fn create_env(
        repo_name: String,
        group_id: Option<u32>,
        project_id: Option<u32>,
        owns_remote: bool,
    ) -> Self {
        let env = TestEnvironment::default();
        let repo = Self {
            env,
            forge_url: Self::forge_url(),
            api_url: Self::api_url(),
            repo_name,
            group_id,
            project_id,
            http: Self::build_client(),
            owns_remote,
        };
        repo.write_netrc();
        repo
    }

    fn forge_url() -> Url {
        Url::parse(GITLAB_FORGE_URL).expect("valid GitLab URL")
    }

    fn api_url() -> Url {
        Self::forge_url()
            .join("/api/v4/")
            .expect("valid GitLab API URL")
    }

    fn build_client() -> Client {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Private-Token {GITLAB_TOKEN}"))
                .expect("valid GitLab authorization header"),
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        Client::builder()
            .default_headers(headers)
            .user_agent("jj-gitlab-integration-test")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build GitLab API client")
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

    fn write_netrc(&self) {
        let host = self
            .forge_url
            .host_str()
            .expect("GitLab URL should have a host");
        fs::write(
            self.env.home_dir().join(".netrc"),
            format!("machine {host} login root password {GITLAB_TOKEN}\n"),
        )
        .expect("write GitLab credentials");
    }

    fn create_remote_repo(&mut self) {
        // Create a group for this test
        let group_response = self.api_call(
            Method::POST,
            "groups",
            Some(&json!({
                "name": self.repo_name,
                "path": self.repo_name.to_lowercase(),
                "visibility": "private",
            })),
        );

        self.group_id = group_response["id"].as_u64().map(|id| id as u32);

        let group_id = self
            .group_id
            .expect("GitLab group creation should return an ID");

        // Create a project in the group
        let project_response = self.api_call(
            Method::POST,
            "projects",
            Some(&json!({
                "name": self.repo_name,
                "namespace_id": group_id,
                "visibility": "private",
                "initialize_with_readme": true,
                "default_branch": "main",
            })),
        );

        self.project_id = project_response["id"].as_u64().map(|id| id as u32);
    }

    fn clone_repo(&self) {
        let _project_id = self
            .project_id
            .expect("project_id should be set when cloning");

        let repo_url = self
            .forge_url
            .join(&format!(
                "/{}/{}.git",
                self.repo_name.to_lowercase(),
                self.repo_name.to_lowercase()
            ))
            .expect("construct GitLab git URL")
            .to_string();

        let work_dir = self.clone_dir();
        work_dir
            .run_jj(["git", "clone", "--colocate", &repo_url, "."])
            .success();
    }

    fn api_call(&self, method: Method, path: &str, body: Option<&Value>) -> Value {
        self.api_call_result(method, path, body)
            .unwrap_or_else(|err| panic!("GitLab API call failed for '{path}': {err}"))
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

impl Drop for GitlabTestRepo {
    fn drop(&mut self) {
        if !self.owns_remote {
            return;
        }

        if let Some(group_id) = self.group_id {
            let _unused = self.api_call_result(Method::DELETE, &format!("groups/{group_id}"), None);
        }
    }
}
