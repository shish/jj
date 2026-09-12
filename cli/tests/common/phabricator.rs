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

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::Value;
use serde_json::json;

use super::TestEnvironment;
use super::TestWorkDir;

const PHABRICATOR_FORGE_URL: &str = "http://phab.localhost:8081";
const PHABRICATOR_API_TOKEN: &str = "cli-aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PHABRICATOR_USER: &str = "admin";
const PHABRICATOR_PASSWORD: &str = "secret";

pub(crate) struct PhabricatorTestRepo {
    env: TestEnvironment,
    forge_url: Url,
    repo_name: String,
    callsign: String,
    repo_url: String,
    http: Client,
    /// Whether dropping this instance should deactivate the remote repository.
    /// Only the instance that created it is responsible for cleaning it up.
    owns_remote: bool,
}

impl PhabricatorTestRepo {
    pub(crate) fn maybe_new() -> Option<Self> {
        if !Self::is_external_tool_installed("arc") {
            eprintln!("Skipping test: `arc` command not found");
            return None;
        }
        if !Self::is_available() {
            eprintln!(
                "Skipping test: Phabricator test server is unavailable at {PHABRICATOR_FORGE_URL}"
            );
            return None;
        }

        Some(Self::new())
    }

    fn new() -> Self {
        let suffix = Self::random_lowercase(4);
        let repo_name = format!("ztst-{suffix}");
        let callsign = format!("ZTST{}", suffix.to_ascii_uppercase());

        let repo = Self::create_env(
            Self::build_client(),
            Self::forge_url(),
            repo_name,
            callsign,
            true,
        );
        repo.create_remote_repo();
        repo.force_create_repository();

        repo.clone_repo();
        repo.write_file(
            ".arcconfig",
            &json!({
                "phabricator.uri": PHABRICATOR_FORGE_URL,
                "repository.callsign": repo.callsign
            })
            .to_string(),
        );
        repo.run("git", &["add", ".arcconfig"]);
        repo.run(
            "git",
            &["commit", "-m", "Initial empty repository", "--allow-empty"],
        );
        repo.run("git", &["push", "origin", "HEAD:master"]);
        repo
    }

    pub(crate) fn fresh_clone(&self) -> Self {
        let repo = Self::create_env(
            self.http.clone(),
            self.forge_url.clone(),
            self.repo_name.clone(),
            self.callsign.clone(),
            false,
        );
        repo.clone_repo();
        repo
    }

    /// Sets up an isolated test environment with Phabricator credentials in
    /// place, without touching the remote yet.
    fn create_env(
        http: Client,
        forge_url: Url,
        repo_name: String,
        callsign: String,
        owns_remote: bool,
    ) -> Self {
        let repo_url = forge_url
            .join(&format!("/source/{repo_name}.git"))
            .expect("construct Phabricator git URL")
            .to_string();

        let env = TestEnvironment::default();
        let repo = Self {
            env,
            forge_url,
            repo_name,
            callsign,
            repo_url,
            http,
            owns_remote,
        };
        repo.write_arcrc();
        repo.write_netrc();
        repo
    }

    fn forge_url() -> Url {
        Url::parse(PHABRICATOR_FORGE_URL).expect("valid Phabricator URL")
    }

    fn build_client() -> Client {
        Client::builder().build().expect("build HTTP client")
    }

    fn is_available() -> bool {
        let Ok(forge_url) = Url::parse(PHABRICATOR_FORGE_URL) else {
            return false;
        };
        let Ok(http) = Client::builder().timeout(Duration::from_secs(5)).build() else {
            return false;
        };
        Self::conduit_request(&http, &forge_url, "user.whoami", Value::Null).is_ok()
    }

    fn is_external_tool_installed(program_name: &str) -> bool {
        Command::new(program_name).arg("--version").output().is_ok()
    }

    fn write_arcrc(&self) {
        let arcrc_path = self.env.home_dir().join(".arcrc");
        std::fs::write(
            &arcrc_path,
            json!({
                "hosts": {
                    format!("{PHABRICATOR_FORGE_URL}/api/"): {
                        "token": PHABRICATOR_API_TOKEN
                    }
                }
            })
            .to_string(),
        )
        .expect("write .arcrc");
        #[cfg(unix)]
        std::fs::set_permissions(&arcrc_path, std::fs::Permissions::from_mode(0o600))
            .expect("set .arcrc permissions");
    }

    fn write_netrc(&self) {
        std::fs::write(
            self.env.home_dir().join(".netrc"),
            format!(
                "machine phab.localhost login {PHABRICATOR_USER} password {PHABRICATOR_PASSWORD}\n"
            ),
        )
        .expect("write .netrc");
    }

    fn create_remote_repo(&self) {
        self.conduit_call(
            "diffusion.repository.edit",
            json!({
                "transactions": [
                    {"type": "name", "value": self.repo_name},
                    {"type": "vcs", "value": "git"},
                    {"type": "callsign", "value": self.callsign},
                    {"type": "status", "value": "active"},
                    {"type": "shortName", "value": self.repo_name}
                ]
            }),
        );
    }

    /// Asks the Phabricator daemon to initialize the repository right away
    /// instead of waiting for the next scheduled update.
    fn force_create_repository(&self) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest_dir
            .parent()
            .expect("cli crate should be inside workspace root");
        let compose_file = repo_root.join("compose.yml");
        let compose_str = compose_file
            .to_str()
            .expect("compose file path should be valid UTF-8");

        let output = Command::new("docker")
            .args([
                "compose",
                "-f",
                compose_str,
                "exec",
                "phabricator",
                "runuser",
                "-u",
                "www-data",
                "bin/repository",
                "update",
                &self.callsign,
            ])
            .output()
            .expect("run docker compose command for phabricator repo update");

        assert!(
            output.status.success(),
            "failed to force-create Phabricator repository {}\nstdout:\n{}\nstderr:\n{}",
            self.callsign,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    fn clone_repo(&self) {
        self.clone_dir()
            .run_jj(["git", "clone", "--colocate", &self.repo_url, "."])
            .success();
    }

    fn conduit_call(&self, method: &str, params: Value) -> Value {
        self.conduit_call_result(method, params)
            .unwrap_or_else(|err| panic!("Phabricator API call failed for '{method}': {err}"))
    }

    fn conduit_call_result(&self, method: &str, params: Value) -> Result<Value, String> {
        Self::conduit_request(&self.http, &self.forge_url, method, params)
    }

    fn conduit_request(
        http: &Client,
        forge_url: &Url,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let api_url = forge_url
            .join(&format!("/api/{method}"))
            .map_err(|err| format!("invalid method URL: {err}"))?;

        let mut params_map = match params {
            Value::Object(map) => map,
            Value::Null => serde_json::Map::new(),
            _ => return Err("params must be an object".to_string()),
        };
        params_map.insert(
            "__conduit__".to_string(),
            json!({"token": PHABRICATOR_API_TOKEN}),
        );
        let params_json = serde_json::to_string(&Value::Object(params_map))
            .map_err(|err| format!("serialize params failed: {err}"))?;

        let response = http
            .post(api_url)
            .form(&[
                ("params", params_json.as_str()),
                ("output", "json"),
                ("__conduit__", "true"),
            ])
            .send()
            .map_err(|err| format!("request failed: {err}"))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|err| format!("read response failed: {err}"))?;
        if !status.is_success() {
            return Err(format!("status {status}: {text}"));
        }
        let value: Value =
            serde_json::from_str(&text).map_err(|err| format!("invalid JSON response: {err}"))?;
        if let Some(code) = value.get("error_code").filter(|code| !code.is_null()) {
            let info = value
                .get("error_info")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(format!("conduit error {code}: {info}"));
        }
        Ok(value["result"].clone())
    }

    fn random_lowercase(len: usize) -> String {
        std::iter::repeat_with(|| {
            loop {
                let value = rand::random::<u8>() & 0b1_1111;
                if value < 26 {
                    break char::from(b'a' + value);
                }
            }
        })
        .take(len)
        .collect()
    }

    fn clone_dir(&self) -> TestWorkDir<'_> {
        self.env.work_dir("").create_dir_all("clone");
        self.env.work_dir("clone")
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

    pub(crate) fn run_jj(&self, args: &[&str]) -> String {
        self.clone_dir().run_jj(args).success().stdout.into_raw()
    }

    pub(crate) fn run_jj_json(&self, args: &[&str]) -> Value {
        let stdout = self.run_jj(args);
        serde_json::from_str(&stdout).unwrap_or_else(|err| {
            panic!(
                "failed to parse JSON from `jj {}` output: {err}\noutput:\n{}",
                args.join(" "),
                stdout
            )
        })
    }

    pub(crate) fn run(&self, program: &str, args: &[&str]) -> String {
        let output = Command::new(program)
            .args(args)
            .current_dir(self.clone_dir().root())
            .env("HOME", self.env.home_dir())
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap_or_else(|err| panic!("failed to run `{program} {}`: {err}", args.join(" ")));

        assert!(
            output.status.success(),
            "`{program} {}` failed\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        String::from_utf8_lossy(&output.stdout).to_string()
    }
}

impl Drop for PhabricatorTestRepo {
    fn drop(&mut self) {
        if !self.owns_remote {
            return;
        }

        let search = self
            .conduit_call_result(
                "diffusion.repository.search",
                json!({"constraints": {"shortNames": [self.repo_name.clone()]}}),
            )
            .ok();

        if let Some(search) = search {
            if let Some(phid) = search["data"]
                .as_array()
                .and_then(|repos| repos.first())
                .and_then(|repo| repo["phid"].as_str())
            {
                let _unused = self.conduit_call_result(
                    "diffusion.repository.edit",
                    json!({
                        "objectIdentifier": phid,
                        "transactions": [{"type": "status", "value": "inactive"}],
                    }),
                );
            }
        }
    }
}
