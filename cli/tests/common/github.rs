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

use std::fs;

use reqwest::Method;
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde_json::Value;
use serde_json::json;

use jj_cli::commands::github::client::GitHubClient;

use super::TestEnvironment;
use super::TestWorkDir;

const GITHUB_FORGE_URL: &str = "https://github.com";
const GITHUB_API_URL: &str = "https://api.github.com";

pub(crate) struct GitHubTestRepo {
    env: TestEnvironment,
    api_url: Url,
    full_name: String,
    token: String,
    http: Client,
    /// Whether dropping this instance should delete the remote repository.
    /// Only the instance that created it is responsible for cleaning it up.
    owns_remote: bool,
}

impl GitHubTestRepo {
    pub(crate) fn maybe_new() -> Option<Self> {
        // Require explicit opt-in to run tests that create real GitHub repositories
        if std::env::var("JJ_TEST_GITHUB_API").is_err() {
            eprintln!(
                "Skipping GitHub integration test: set JJ_TEST_GITHUB_API=1 to enable tests \
                 that create real GitHub repositories"
            );
            return None;
        }

        let forge_url = Url::parse(GITHUB_FORGE_URL).expect("valid GitHub URL");
        let token = match GitHubClient::resolve_token(&forge_url) {
            Ok(token) => token,
            Err(_) => {
                eprintln!("Skipping GitHub integration test: no token configured for github.com");
                return None;
            }
        };
        Some(Self::new(token))
    }

    fn new(token: String) -> Self {
        let api_url = Url::parse(GITHUB_API_URL).expect("valid GitHub API URL");
        let http = Self::build_client(&token);
        let mut test_repo = Self::create_env(api_url, String::new(), token, http, true);
        test_repo.write_netrc();
        let repo_url = test_repo.create_remote_repo();
        test_repo
            .clone_dir()
            .run_jj(["git", "clone", "--colocate", &repo_url, "."])
            .success();
        test_repo
    }

    #[expect(dead_code)]
    pub(crate) fn fresh_clone(&self) -> Self {
        let api_url = Url::parse(GITHUB_API_URL).expect("valid GitHub API URL");
        let http = Self::build_client(&self.token);
        let test_repo = Self::create_env(
            api_url,
            self.full_name.clone(),
            self.token.clone(),
            http,
            false,
        );
        test_repo.write_netrc();
        let repo_url = format!("https://github.com/{}.git", test_repo.full_name);
        test_repo
            .clone_dir()
            .run_jj(["git", "clone", &repo_url, "."])
            .success();
        test_repo
    }

    /// Sets up an isolated test environment with GitHub credentials in place,
    /// without touching the remote yet.
    fn create_env(
        api_url: Url,
        full_name: String,
        token: String,
        http: Client,
        owns_remote: bool,
    ) -> Self {
        let env = TestEnvironment::default();
        Self {
            env,
            api_url,
            full_name,
            token,
            http,
            owns_remote,
        }
    }

    fn build_client(token: &str) -> Client {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .expect("valid GitHub authorization header"),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        Client::builder()
            .default_headers(headers)
            .user_agent("jj-github-integration-test")
            .build()
            .expect("build GitHub API client")
    }

    fn write_netrc(&self) {
        fs::write(
            self.env.home_dir().join(".netrc"),
            format!(
                "machine github.com login x-access-token password {}\n",
                self.token
            ),
        )
        .expect("write GitHub credentials");
    }

    /// Creates a fresh private repository on GitHub, records its full name, and
    /// returns its clone URL.
    fn create_remote_repo(&mut self) -> String {
        let repo_name = format!("jj-integration-{:08x}", rand::random::<u32>());
        let repo = self.api_call(
            Method::POST,
            "user/repos",
            Some(&json!({
                "name": repo_name,
                "private": true,
                "auto_init": true,
            })),
        );
        self.full_name = repo["full_name"]
            .as_str()
            .expect("GitHub repository response should include full_name")
            .to_string();
        repo["clone_url"]
            .as_str()
            .expect("GitHub repository response should include clone_url")
            .to_string()
    }

    fn api_call(&self, method: Method, path: &str, body: Option<&Value>) -> Value {
        self.api_call_result(method, path, body)
            .unwrap_or_else(|err| panic!("GitHub API call failed for '{path}': {err}"))
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

    #[expect(dead_code)]
    pub(crate) fn write_file(&self, relative_path: &str, content: &str) {
        self.clone_dir().write_file(relative_path, content);
    }

    #[expect(dead_code)]
    pub(crate) fn read_file(&self, relative_path: &str) -> String {
        self.clone_dir().read_file(relative_path).to_string()
    }

    #[expect(dead_code)]
    pub(crate) fn file_exists(&self, relative_path: &str) -> bool {
        self.clone_dir().root().join(relative_path).exists()
    }
}

impl Drop for GitHubTestRepo {
    fn drop(&mut self) {
        if !self.owns_remote {
            return;
        }
        let _unused =
            self.api_call_result(Method::DELETE, &format!("repos/{}", self.full_name), None);
    }
}

pub(crate) fn maybe_external_github_repo_url() -> Option<String> {
    let repo_url = std::env::var("JJ_INTEGRATION_GITHUB_REPO_URL")
        .ok()
        .filter(|url| !url.trim().is_empty());
    let Some(repo_url) = repo_url else {
        eprintln!("Skipping GitHub integration test: JJ_INTEGRATION_GITHUB_REPO_URL is not set");
        return None;
    };
    Some(repo_url)
}

pub(crate) fn cloned_repo() -> Option<TestEnvironment> {
    let repo_url = maybe_external_github_repo_url()?;
    let env = TestEnvironment::default();
    env.work_dir("repo")
        .run_jj(["git", "clone", "--colocate", repo_url.as_str(), "."])
        .success();
    Some(env)
}

pub(crate) fn run_jj<I, S>(env: &TestEnvironment, args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    env.work_dir("repo")
        .run_jj(args)
        .success()
        .stdout
        .into_raw()
}

pub(crate) fn run_jj_json<I, S>(env: &TestEnvironment, args: I) -> Value
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = run_jj(env, args);
    serde_json::from_str(&output)
        .unwrap_or_else(|err| panic!("failed to parse jj output as JSON: {err}\noutput:\n{output}"))
}

pub(crate) fn repo_path(env: &TestEnvironment) -> std::path::PathBuf {
    // The repo is cloned into "repo" subdirectory within the test environment
    env.work_dir("repo").root().to_path_buf()
}
