//! GitHub host command boundary.

use futures_util::future::BoxFuture;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// A recorded GitHub command invocation with secrets redacted.
#[derive(Debug, Clone, PartialEq)]
pub struct GhCommandInvocation {
    pub method: String,
    pub args: Vec<String>,
    pub body: Option<Value>,
}

/// Error returned by the GitHub command boundary.
#[derive(Debug, Clone)]
pub enum GhCommandError {
    Spawn { summary: String },
    NonZeroExit { code: Option<i32>, stderr: String },
    InvalidResponse { summary: String },
    Io { summary: String },
}

impl GhCommandError {
    /// Machine-readable command error code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Spawn { .. } => "spawn_error",
            Self::NonZeroExit { .. } => "non_zero_exit",
            Self::InvalidResponse { .. } => "invalid_response",
            Self::Io { .. } => "io_error",
        }
    }
}

impl std::fmt::Display for GhCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn { summary } => write!(formatter, "GitHub command spawn failed: {summary}"),
            Self::NonZeroExit { code, stderr } => {
                write!(formatter, "GitHub command exited {code:?}: {stderr}")
            }
            Self::InvalidResponse { summary } => {
                write!(formatter, "GitHub command response was invalid: {summary}")
            }
            Self::Io { summary } => write!(formatter, "GitHub command I/O failed: {summary}"),
        }
    }
}

impl std::error::Error for GhCommandError {}

/// Parsed authentication status from `gh auth status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhAuthStatus {
    pub authenticated: bool,
    pub account: Option<String>,
}

/// Async boundary for host `gh` and `git` commands.
pub trait GhCommand: Clone + Send + Sync + 'static {
    fn version(&self) -> BoxFuture<'_, Result<String, GhCommandError>>;

    fn auth_status(
        &self,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<GhAuthStatus, GhCommandError>>;

    fn api_get(
        &self,
        endpoint: &str,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>>;

    fn api_post(
        &self,
        endpoint: &str,
        body: Value,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>>;

    fn api_put(
        &self,
        endpoint: &str,
        body: Value,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>>;

    fn api_patch(
        &self,
        endpoint: &str,
        body: Value,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>>;

    fn api_delete(
        &self,
        endpoint: &str,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<(), GhCommandError>>;

    fn search_code(
        &self,
        query: &str,
        token: Option<&SecretString>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>>;

    fn clone_repo(
        &self,
        repo: &str,
        dest: &Path,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<String, GhCommandError>>;

    fn checkout_ref(
        &self,
        repo_path: &Path,
        ref_name: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>>;

    fn git_stage(
        &self,
        repo_path: &Path,
        paths: &[String],
    ) -> BoxFuture<'_, Result<(), GhCommandError>>;

    fn git_commit(
        &self,
        repo_path: &Path,
        message: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>>;

    fn branch_create(
        &self,
        repo_path: &Path,
        branch: &str,
        from_ref: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>>;

    fn git_push(
        &self,
        repo_path: &Path,
        remote: &str,
        refspec: &str,
        force: bool,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<(), GhCommandError>>;
}

/// Tokio-backed host `gh` and `git` command runner.
#[derive(Debug, Clone)]
pub struct TokioGhCommand {
    gh_bin: PathBuf,
    git_bin: PathBuf,
}

impl TokioGhCommand {
    #[must_use]
    pub fn new(gh_bin: impl Into<PathBuf>, git_bin: impl Into<PathBuf>) -> Self {
        Self {
            gh_bin: gh_bin.into(),
            git_bin: git_bin.into(),
        }
    }
}

impl Default for TokioGhCommand {
    fn default() -> Self {
        Self::new("gh", "git")
    }
}

impl GhCommand for TokioGhCommand {
    fn version(&self) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let gh_bin = self.gh_bin.clone();
        Box::pin(async move {
            let output = command_output(Command::new(gh_bin).arg("--version")).await?;
            Ok(output.lines().next().unwrap_or_default().trim().to_owned())
        })
    }

    fn auth_status(
        &self,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<GhAuthStatus, GhCommandError>> {
        let gh_bin = self.gh_bin.clone();
        let token = token.cloned();
        Box::pin(async move {
            let mut command = Command::new(gh_bin);
            apply_gh_token(&mut command, token.as_ref());
            let output = command
                .arg("auth")
                .arg("status")
                .output()
                .await
                .map_err(spawn_error)?;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let authenticated = output.status.success() && combined.contains("Logged in to");
            Ok(GhAuthStatus {
                authenticated,
                account: parse_auth_account(&combined),
            })
        })
    }

    fn api_get(
        &self,
        endpoint: &str,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        let endpoint = endpoint.to_owned();
        let command = self.clone();
        let token = token.cloned();
        Box::pin(async move {
            command
                .api_json("GET", &endpoint, None, token.as_ref())
                .await
        })
    }

    fn api_post(
        &self,
        endpoint: &str,
        body: Value,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        let endpoint = endpoint.to_owned();
        let command = self.clone();
        let token = token.cloned();
        Box::pin(async move {
            command
                .api_json("POST", &endpoint, Some(body), token.as_ref())
                .await
        })
    }

    fn api_put(
        &self,
        endpoint: &str,
        body: Value,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        let endpoint = endpoint.to_owned();
        let command = self.clone();
        let token = token.cloned();
        Box::pin(async move {
            command
                .api_json("PUT", &endpoint, Some(body), token.as_ref())
                .await
        })
    }

    fn api_patch(
        &self,
        endpoint: &str,
        body: Value,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        let endpoint = endpoint.to_owned();
        let command = self.clone();
        let token = token.cloned();
        Box::pin(async move {
            command
                .api_json("PATCH", &endpoint, Some(body), token.as_ref())
                .await
        })
    }

    fn api_delete(
        &self,
        endpoint: &str,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<(), GhCommandError>> {
        let endpoint = endpoint.to_owned();
        let command = self.clone();
        let token = token.cloned();
        Box::pin(async move {
            command
                .api_json("DELETE", &endpoint, None, token.as_ref())
                .await
                .map(|_| ())
        })
    }

    fn search_code(
        &self,
        query: &str,
        token: Option<&SecretString>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        let gh_bin = self.gh_bin.clone();
        let query = query.to_owned();
        let token = token.cloned();
        Box::pin(async move {
            let mut command = Command::new(gh_bin);
            apply_gh_token(&mut command, token.as_ref());
            let output = command_output(
                command
                    .arg("search")
                    .arg("code")
                    .arg(query)
                    .arg("--json")
                    .arg("path,repository,textMatches")
                    .arg("--limit")
                    .arg(limit.to_string()),
            )
            .await?;
            parse_json(&output)
        })
    }

    fn clone_repo(
        &self,
        repo: &str,
        dest: &Path,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let gh_bin = self.gh_bin.clone();
        let git_bin = self.git_bin.clone();
        let repo = repo.to_owned();
        let dest = dest.to_path_buf();
        let token = token.cloned();
        Box::pin(async move {
            if let Some(token) = token.as_ref() {
                let clone_url = format!(
                    "https://x-access-token:{}@github.com/{repo}.git",
                    token.expose_secret()
                );
                let public_url = format!("https://github.com/{repo}.git");
                let mut clone = Command::new(&git_bin);
                apply_gh_token(&mut clone, Some(token));
                command_output_redacted(clone.arg("clone").arg(clone_url).arg(&dest), Some(token))
                    .await?;

                command_output(
                    Command::new(&git_bin)
                        .arg("-C")
                        .arg(&dest)
                        .arg("remote")
                        .arg("set-url")
                        .arg("origin")
                        .arg(public_url),
                )
                .await?;
            } else {
                command_output(
                    Command::new(gh_bin)
                        .arg("repo")
                        .arg("clone")
                        .arg(repo)
                        .arg(&dest),
                )
                .await?;
            }
            rev_parse_head(&git_bin, &dest).await
        })
    }

    fn checkout_ref(
        &self,
        repo_path: &Path,
        ref_name: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let git_bin = self.git_bin.clone();
        let repo_path = repo_path.to_path_buf();
        let ref_name = ref_name.to_owned();
        Box::pin(async move {
            command_output(
                Command::new(&git_bin)
                    .arg("-C")
                    .arg(&repo_path)
                    .arg("checkout")
                    .arg(ref_name),
            )
            .await?;
            rev_parse_head(&git_bin, &repo_path).await
        })
    }

    fn git_stage(
        &self,
        repo_path: &Path,
        paths: &[String],
    ) -> BoxFuture<'_, Result<(), GhCommandError>> {
        let git_bin = self.git_bin.clone();
        let repo_path = repo_path.to_path_buf();
        let paths = paths.to_vec();
        Box::pin(async move {
            let mut command = Command::new(git_bin);
            command.arg("-C").arg(repo_path).arg("add").args(paths);
            command_output(&mut command).await.map(|_| ())
        })
    }

    fn git_commit(
        &self,
        repo_path: &Path,
        message: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let git_bin = self.git_bin.clone();
        let repo_path = repo_path.to_path_buf();
        let message = message.to_owned();
        Box::pin(async move {
            command_output(
                Command::new(&git_bin)
                    .arg("-C")
                    .arg(&repo_path)
                    .arg("commit")
                    .arg("-m")
                    .arg(message),
            )
            .await?;
            let sha = command_output(
                Command::new(git_bin)
                    .arg("-C")
                    .arg(repo_path)
                    .arg("rev-parse")
                    .arg("HEAD"),
            )
            .await?;
            Ok(sha.trim().to_owned())
        })
    }

    fn branch_create(
        &self,
        repo_path: &Path,
        branch: &str,
        from_ref: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let git_bin = self.git_bin.clone();
        let repo_path = repo_path.to_path_buf();
        let branch = branch.to_owned();
        let from_ref = from_ref.to_owned();
        Box::pin(async move {
            command_output(
                Command::new(&git_bin)
                    .arg("-C")
                    .arg(&repo_path)
                    .arg("checkout")
                    .arg("-b")
                    .arg(branch)
                    .arg(from_ref),
            )
            .await?;
            rev_parse_head(&git_bin, &repo_path).await
        })
    }

    fn git_push(
        &self,
        repo_path: &Path,
        remote: &str,
        refspec: &str,
        force: bool,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<(), GhCommandError>> {
        let git_bin = self.git_bin.clone();
        let repo_path = repo_path.to_path_buf();
        let remote = remote.to_owned();
        let refspec = refspec.to_owned();
        let token = token.cloned();
        Box::pin(async move {
            let mut command = Command::new(git_bin);
            apply_gh_token(&mut command, token.as_ref());
            command.arg("-C").arg(repo_path);
            if token.is_some() {
                command
                    .arg("-c")
                    .arg("credential.helper=!gh auth git-credential");
            }
            command.arg("push").arg(remote).arg(refspec);
            if force {
                command.arg("--force");
            }
            command_output(&mut command).await.map(|_| ())
        })
    }
}

impl TokioGhCommand {
    async fn api_json(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<Value>,
        token: Option<&SecretString>,
    ) -> Result<Value, GhCommandError> {
        let mut command = Command::new(&self.gh_bin);
        apply_gh_token(&mut command, token);
        command.arg("api").arg(endpoint).arg("--method").arg(method);
        if body.is_some() {
            command.arg("--input").arg("-");
            command.stdin(Stdio::piped());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(spawn_error)?;
        if let Some(body) = body {
            let mut stdin = child.stdin.take().ok_or_else(|| GhCommandError::Io {
                summary: "gh api stdin unavailable".to_owned(),
            })?;
            stdin
                .write_all(body.to_string().as_bytes())
                .await
                .map_err(io_error)?;
            drop(stdin);
        }
        let output = child.wait_with_output().await.map_err(io_error)?;
        if !output.status.success() {
            return Err(GhCommandError::NonZeroExit {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        let stdout =
            String::from_utf8(output.stdout).map_err(|_| GhCommandError::InvalidResponse {
                summary: "gh api output was not utf-8".to_owned(),
            })?;
        if stdout.trim().is_empty() {
            Ok(Value::Null)
        } else {
            parse_json(&stdout)
        }
    }
}

/// Deterministic fake command for tests.
#[derive(Clone, Default)]
pub struct FakeGhCommand {
    state: Arc<Mutex<FakeGhState>>,
}

#[derive(Default)]
struct FakeGhState {
    version: String,
    version_error: Option<GhCommandError>,
    auth_status: Option<GhAuthStatus>,
    invocations: Vec<GhCommandInvocation>,
    fixtures: HashMap<String, Value>,
}

impl std::fmt::Debug for FakeGhCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FakeGhCommand { secret buffers redacted }")
    }
}

impl FakeGhCommand {
    #[must_use]
    pub fn invocations(&self) -> Vec<GhCommandInvocation> {
        self.state
            .lock()
            .map(|state| state.invocations.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn api_get_count(&self) -> usize {
        self.invocations()
            .iter()
            .filter(|invocation| invocation.method == "api_get")
            .count()
    }

    #[must_use]
    pub fn clone_count(&self) -> usize {
        self.invocations()
            .iter()
            .filter(|invocation| invocation.method == "clone_repo")
            .count()
    }

    pub fn put_api_response(&self, method: &str, endpoint: &str, value: Value) {
        self.put_response(&fixture_key(method, &[endpoint.to_owned()]), value);
    }

    pub fn put_response(&self, key: &str, value: Value) {
        if let Ok(mut state) = self.state.lock() {
            state.fixtures.insert(key.to_owned(), value);
        }
    }

    pub fn set_version(&self, version: impl Into<String>) {
        if let Ok(mut state) = self.state.lock() {
            state.version = version.into();
        }
    }

    pub fn set_version_error(&self, error: GhCommandError) {
        if let Ok(mut state) = self.state.lock() {
            state.version_error = Some(error);
        }
    }

    pub fn set_auth_status(&self, auth_status: GhAuthStatus) {
        if let Ok(mut state) = self.state.lock() {
            state.auth_status = Some(auth_status);
        }
    }
}

impl GhCommand for FakeGhCommand {
    fn version(&self) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let result = self
            .state
            .lock()
            .map(|mut state| {
                state
                    .invocations
                    .push(invocation("version", Vec::new(), None));
                if let Some(error) = state.version_error.clone() {
                    return Err(error);
                }
                if state.version.is_empty() {
                    Ok("gh version 2.72.0 (fake)".to_owned())
                } else {
                    Ok(state.version.clone())
                }
            })
            .map_err(|_| fake_unavailable())
            .and_then(std::convert::identity);
        Box::pin(async move { result })
    }

    fn auth_status(
        &self,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<GhAuthStatus, GhCommandError>> {
        let result = self
            .state
            .lock()
            .map(|mut state| {
                state
                    .invocations
                    .push(invocation("auth_status", Vec::new(), None));
                state.auth_status.clone().unwrap_or(GhAuthStatus {
                    authenticated: true,
                    account: Some("fake-otto".to_owned()),
                })
            })
            .map_err(|_| fake_unavailable());
        Box::pin(async move { result })
    }

    fn api_get(
        &self,
        endpoint: &str,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        let endpoint = endpoint.to_owned();
        self.fake_value("api_get", vec![endpoint.clone()], None, move || {
            if is_repo_metadata_endpoint(&endpoint) {
                json!({ "size": 1 })
            } else {
                Value::Null
            }
        })
    }

    fn api_post(
        &self,
        endpoint: &str,
        body: Value,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        self.fake_value("api_post", vec![endpoint.to_owned()], Some(body), || {
            Value::Null
        })
    }

    fn api_put(
        &self,
        endpoint: &str,
        body: Value,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        self.fake_value("api_put", vec![endpoint.to_owned()], Some(body), || {
            Value::Null
        })
    }

    fn api_patch(
        &self,
        endpoint: &str,
        body: Value,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        self.fake_value("api_patch", vec![endpoint.to_owned()], Some(body), || {
            Value::Null
        })
    }

    fn api_delete(
        &self,
        endpoint: &str,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<(), GhCommandError>> {
        let result = self
            .record("api_delete", vec![endpoint.to_owned()], None)
            .map(|_| ());
        Box::pin(async move { result })
    }

    fn search_code(
        &self,
        query: &str,
        _token: Option<&SecretString>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>> {
        self.fake_value(
            "search_code",
            vec![query.to_owned(), limit.to_string()],
            None,
            || Value::Array(Vec::new()),
        )
    }

    fn clone_repo(
        &self,
        repo: &str,
        dest: &Path,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let result = self
            .record(
                "clone_repo",
                vec![repo.to_owned(), dest.display().to_string()],
                None,
            )
            .and_then(|value| {
                std::fs::create_dir_all(dest).map_err(io_error)?;
                std::fs::write(dest.join("README.md"), "synthetic checkout\n").map_err(io_error)?;
                Ok(value
                    .as_str()
                    .unwrap_or("0123456789abcdef0123456789abcdef01234567")
                    .to_owned())
            });
        Box::pin(async move { result })
    }

    fn checkout_ref(
        &self,
        repo_path: &Path,
        ref_name: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let result = self
            .record(
                "checkout_ref",
                vec![repo_path.display().to_string(), ref_name.to_owned()],
                None,
            )
            .and_then(|value| {
                Ok(value
                    .as_str()
                    .unwrap_or("0123456789abcdef0123456789abcdef01234567")
                    .to_owned())
            });
        Box::pin(async move { result })
    }

    fn git_stage(
        &self,
        repo_path: &Path,
        paths: &[String],
    ) -> BoxFuture<'_, Result<(), GhCommandError>> {
        let mut args = vec![repo_path.display().to_string()];
        args.extend(paths.iter().cloned());
        let result = self.record("git_stage", args, None).map(|_| ());
        Box::pin(async move { result })
    }

    fn git_commit(
        &self,
        repo_path: &Path,
        message: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let result = self
            .record(
                "git_commit",
                vec![repo_path.display().to_string(), message.to_owned()],
                None,
            )
            .and_then(|value| {
                Ok(value
                    .as_str()
                    .unwrap_or("0123456789abcdef0123456789abcdef01234567")
                    .to_owned())
            });
        Box::pin(async move { result })
    }

    fn branch_create(
        &self,
        repo_path: &Path,
        branch: &str,
        from_ref: &str,
    ) -> BoxFuture<'_, Result<String, GhCommandError>> {
        let result = self
            .record(
                "branch_create",
                vec![
                    repo_path.display().to_string(),
                    branch.to_owned(),
                    from_ref.to_owned(),
                ],
                None,
            )
            .and_then(|value| {
                Ok(value
                    .as_str()
                    .unwrap_or("0123456789abcdef0123456789abcdef01234567")
                    .to_owned())
            });
        Box::pin(async move { result })
    }

    fn git_push(
        &self,
        repo_path: &Path,
        remote: &str,
        refspec: &str,
        force: bool,
        _token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<(), GhCommandError>> {
        let result = self
            .record(
                "git_push",
                vec![
                    repo_path.display().to_string(),
                    remote.to_owned(),
                    refspec.to_owned(),
                    force.to_string(),
                ],
                None,
            )
            .map(|_| ());
        Box::pin(async move { result })
    }
}

impl FakeGhCommand {
    fn fake_value<F>(
        &self,
        method: &str,
        args: Vec<String>,
        body: Option<Value>,
        fallback: F,
    ) -> BoxFuture<'_, Result<Value, GhCommandError>>
    where
        F: FnOnce() -> Value + Send + 'static,
    {
        let result = self
            .record(method, args, body)
            .map(|value| if value.is_null() { fallback() } else { value });
        Box::pin(async move { result })
    }

    fn record(
        &self,
        method: &str,
        args: Vec<String>,
        body: Option<Value>,
    ) -> Result<Value, GhCommandError> {
        let key = fixture_key(method, &args);
        self.state
            .lock()
            .map_err(|_| fake_unavailable())
            .map(|mut state| {
                state.invocations.push(invocation(method, args, body));
                state.fixtures.get(&key).cloned().unwrap_or(Value::Null)
            })
    }
}

fn invocation(method: &str, args: Vec<String>, body: Option<Value>) -> GhCommandInvocation {
    GhCommandInvocation {
        method: method.to_owned(),
        args,
        body,
    }
}

fn fixture_key(method: &str, args: &[String]) -> String {
    format!("{method}:{}", args.join("|"))
}

fn is_repo_metadata_endpoint(endpoint: &str) -> bool {
    let parts = endpoint
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    matches!(parts.as_slice(), ["repos", owner, repo] if !owner.is_empty() && !repo.is_empty())
}

fn apply_gh_token(command: &mut Command, token: Option<&SecretString>) {
    if let Some(token) = token {
        command.env("GH_TOKEN", token.expose_secret());
    }
}

fn parse_auth_account(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some((_, account)) = line.split_once("account ") {
            return account
                .split_whitespace()
                .next()
                .map(|account| account.trim_matches(['(', ')']).to_owned())
                .filter(|account| !account.is_empty());
        }
        if let Some((_, account)) = line.split_once(" as ") {
            return account
                .split_whitespace()
                .next()
                .map(|account| account.trim_matches(['(', ')']).to_owned())
                .filter(|account| !account.is_empty());
        }
    }
    None
}

async fn command_output(command: &mut Command) -> Result<String, GhCommandError> {
    let output = command.output().await.map_err(spawn_error)?;
    if !output.status.success() {
        return Err(GhCommandError::NonZeroExit {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    String::from_utf8(output.stdout).map_err(|_| GhCommandError::InvalidResponse {
        summary: "command output was not utf-8".to_owned(),
    })
}

async fn command_output_redacted(
    command: &mut Command,
    token: Option<&SecretString>,
) -> Result<String, GhCommandError> {
    command_output(command)
        .await
        .map_err(|error| redact_error(error, token))
}

fn redact_error(error: GhCommandError, token: Option<&SecretString>) -> GhCommandError {
    let Some(token) = token else {
        return error;
    };
    let secret = token.expose_secret();
    match error {
        GhCommandError::NonZeroExit { code, stderr } => GhCommandError::NonZeroExit {
            code,
            stderr: stderr.replace(secret, "<redacted>"),
        },
        GhCommandError::Spawn { summary } => GhCommandError::Spawn {
            summary: summary.replace(secret, "<redacted>"),
        },
        GhCommandError::InvalidResponse { summary } => GhCommandError::InvalidResponse {
            summary: summary.replace(secret, "<redacted>"),
        },
        GhCommandError::Io { summary } => GhCommandError::Io {
            summary: summary.replace(secret, "<redacted>"),
        },
    }
}

async fn rev_parse_head(git_bin: &Path, repo_path: &Path) -> Result<String, GhCommandError> {
    let sha = command_output(
        Command::new(git_bin)
            .arg("-C")
            .arg(repo_path)
            .arg("rev-parse")
            .arg("HEAD"),
    )
    .await?;
    Ok(sha.trim().to_owned())
}

fn parse_json(output: &str) -> Result<Value, GhCommandError> {
    serde_json::from_str(output).map_err(|error| GhCommandError::InvalidResponse {
        summary: format!("JSON parse failed: {error}"),
    })
}

fn spawn_error(error: std::io::Error) -> GhCommandError {
    GhCommandError::Spawn {
        summary: error.to_string(),
    }
}

fn io_error(error: std::io::Error) -> GhCommandError {
    GhCommandError::Io {
        summary: error.to_string(),
    }
}

fn fake_unavailable() -> GhCommandError {
    GhCommandError::InvalidResponse {
        summary: "fake GitHub command state unavailable".to_owned(),
    }
}
