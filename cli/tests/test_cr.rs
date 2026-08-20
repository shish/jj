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

use crate::common::TestEnvironment;

fn _setup() -> TestEnvironment {
    let test_env = TestEnvironment::default();
    test_env
        .run_jj_in(".", ["git", "init", "--colocate", "repo"])
        .success();
    test_env.add_config(r#"cr.forge = "demo""#);
    let work_dir = test_env.work_dir("repo");
    work_dir.write_file(
        ".git/config",
        "[remote \"origin\"]\nurl = https://github.com/foo/bar.git\n",
    );
    test_env
}

#[test]
fn test_remote_url_detection() {
    let test_env = TestEnvironment::default();
    test_env
        .run_jj_in(".", ["git", "init", "--colocate", "repo"])
        .success();
    let work_dir = test_env.work_dir("repo");

    work_dir.write_file(
        ".git/config",
        "[remote \"origin\"]\nurl = https://github.com/foo/bar.git\n",
    );
    let output = test_env.work_dir("repo").run_jj(["cr", "info"]);
    insta::assert_snapshot!(output, @r#"
    Remote name: RemoteNameBuf("origin")
    CR backend:  GitHub
    Forge URL:   https://github.com/
    [EOF]
    ------- stderr -------
    Done importing changes from the underlying Git repo.
    [EOF]
    "#);
}

#[test]
fn test_remote_name_detection() {
    let test_env = TestEnvironment::default();
    test_env
        .run_jj_in(".", ["git", "init", "--colocate", "repo"])
        .success();
    let work_dir = test_env.work_dir("repo");

    work_dir.write_file(
        ".git/config",
        "[remote \"demo\"]\nurl = https://code.mycorp.com/bar.git\n",
    );
    let output = test_env.work_dir("repo").run_jj(["cr", "info"]);
    insta::assert_snapshot!(output, @r#"
    Remote name: RemoteNameBuf("demo")
    CR backend:  Demo
    Forge URL:   https://code.mycorp.com/
    [EOF]
    ------- stderr -------
    Done importing changes from the underlying Git repo.
    Hint: Working with the only existing remote: demo
    Hint: Working with the only existing remote: demo
    [EOF]
    "#);
}

#[test]
fn test_cr_info() {
    let test_env = _setup();
    let output = test_env.work_dir("repo").run_jj(["cr", "info"]);
    insta::assert_snapshot!(output, @r#"
    Remote name: RemoteNameBuf("origin")
    CR backend:  Demo
    Forge URL:   https://github.com/
    [EOF]
    ------- stderr -------
    Done importing changes from the underlying Git repo.
    [EOF]
    "#);
}

#[test]
fn test_cr_list_table() {
    let test_env = _setup();
    let output = test_env.work_dir("repo").run_jj(["cr", "list"]);
    insta::assert_snapshot!(output, @"
    ┌─────┬───────────────────────┬──────────────┬─────────────────────┬──────────┐
    │ ID  ┆ Title                 ┆ State        ┆ Checks              ┆ Comments │
    ╞═════╪═══════════════════════╪══════════════╪═════════════════════╪══════════╡
    │ 123 ┆ Demo Review           ┆ Accepted     ┆ Lint, Build         ┆ 0        │
    │ 456 ┆ Demo Review 2         ┆ Needs Review ┆ Lint, Build, Deploy ┆ 2        │
    │ 789 ┆ Demo Review with a    ┆ Needs Review ┆ ✓7, …6, ✗7          ┆ 2        │
    │     ┆ longer title to check ┆              ┆                     ┆          │
    │     ┆ longer titles         ┆              ┆                     ┆          │
    └─────┴───────────────────────┴──────────────┴─────────────────────┴──────────┘
    [EOF]
    ------- stderr -------
    Done importing changes from the underlying Git repo.
    [EOF]
    ");
}

#[test]
fn test_cr_list_json() {
    let test_env = _setup();
    let output = test_env.work_dir("repo").run_jj(["cr", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output.stdout.into_raw()).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 3);
}

#[test]
fn test_cr_log() {
    let test_env = _setup();
    let output = test_env.work_dir("repo").run_jj(["cr", "log"]);
    insta::assert_snapshot!(output, @"
    @  qpvuntsm test.user@example.com 2001-02-03 08:05:07 e8849ae1 Accepted ✓ …
    │  (empty) (no description set)
    ◆  zzzzzzzz root() 00000000
    [EOF]
    ------- stderr -------
    Done importing changes from the underlying Git repo.
    [EOF]
    ");
}
