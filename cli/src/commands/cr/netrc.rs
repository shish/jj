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

use crate::command_error::CommandError;
use crate::command_error::user_error;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Reads a `login`/`password` pair for `host` from `~/.netrc`.
pub fn netrc_read(host: &str) -> Result<Option<(String, String)>, CommandError> {
    let home = etcetera::home_dir()
        .map_err(|e| user_error(format!("Error getting home directory: {}", e)))?;
    let path = home.join(".netrc");

    // if file doesn't exist, silently return None
    if !std::path::Path::new(&path).exists() {
        return Ok(None);
    }

    // If file exists but is insecure, return None loudly
    #[cfg(unix)]
    if std::fs::metadata(&path)
        .ok()
        .map_or(false, |m| m.mode() & 0o077 != 0)
    {
        return Err(user_error(format!(
            "~/.netrc has insecure permissions set (should be 0600) - refusing to read"
        )));
    }

    let content = std::fs::read_to_string(path)?;
    Ok(netrc_parse(&content, host))
}

/// Parses a `.netrc` file for `host` and returns the `login`/`password` pair.
fn netrc_parse(content: &str, host: &str) -> Option<(String, String)> {
    // Strip comments so they don't interfere with tokenization.
    let uncommented: String = content
        .lines()
        .map(|line| line.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let tokens = shlex::split(&uncommented)?;

    let mut matched = false;
    let mut login: Option<String> = None;
    let mut password: Option<String> = None;
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "machine" => {
                if matched {
                    break;
                }
                i += 1;
                matched = tokens.get(i).map(String::as_str) == Some(host);
            }
            "default" => {
                if matched {
                    break;
                }
                matched = true;
            }
            "login" if matched => {
                i += 1;
                login = tokens.get(i).cloned();
            }
            "password" if matched => {
                i += 1;
                password = tokens.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    match (login, password) {
        (Some(login), Some(password)) if !password.is_empty() => Some((login, password)),
        _ => None,
    }
}

#[cfg(test)]
mod netrc_tests {
    use super::*;

    #[test]
    fn test_netrc_parse() {
        let content = "machine github.com login foo password bar";
        assert_eq!(
            netrc_parse(content, "github.com"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn test_netrc_parse_no_password() {
        let content = "machine github.com login foo";
        assert_eq!(netrc_parse(content, "github.com"), None);
    }

    #[test]
    fn test_netrc_parse_newlines() {
        let content = "machine github.com\nlogin foo\npassword bar";
        assert_eq!(
            netrc_parse(content, "github.com"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn test_netrc_parse_comments() {
        let content = "# my github login\nmachine github.com\nlogin foo\n# todo: keep this secret!\npassword bar";
        assert_eq!(
            netrc_parse(content, "github.com"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }
}
