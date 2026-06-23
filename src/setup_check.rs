//! GitHub setup-check result builder.

use crate::command::GhCommand;
use futures_util::future::BoxFuture;
use secrecy::SecretString;
use serde_json::{Value, json};

/// GitHub setup status values surfaced to configuration UIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubSetupStatus {
    Ok,
    GhMissing,
    NotAuthenticated,
}

impl GithubSetupStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::GhMissing => "gh_missing",
            Self::NotAuthenticated => "not_authenticated",
        }
    }
}

/// Structured setup-check result for the GitHub package.
#[derive(Debug, Clone, PartialEq)]
pub struct GithubSetupCheckResult {
    pub status: GithubSetupStatus,
    pub ok: bool,
    pub message: Option<String>,
    pub details: Value,
}

/// GitHub setup check runner.
#[derive(Debug, Clone)]
pub struct GithubSetupCheck<C> {
    command: C,
}

impl<C> GithubSetupCheck<C>
where
    C: GhCommand,
{
    #[must_use]
    pub fn new(command: C) -> Self {
        Self { command }
    }

    pub fn run(
        &self,
        token: Option<&SecretString>,
    ) -> BoxFuture<'_, Result<GithubSetupCheckResult, crate::command::GhCommandError>> {
        let check = self.clone();
        let token = token.cloned();
        Box::pin(async move { check.run_inner(token.as_ref()).await })
    }

    async fn run_inner(
        self,
        token: Option<&SecretString>,
    ) -> Result<GithubSetupCheckResult, crate::command::GhCommandError> {
        let auth_mode = if token.is_some() { "token" } else { "host" };
        let version = match self.command.version().await {
            Ok(version) if !version.trim().is_empty() => version,
            Ok(_) => {
                return Ok(result(
                    GithubSetupStatus::GhMissing,
                    Some("gh CLI not found on PATH; install GitHub CLI".to_owned()),
                    "unavailable",
                    false,
                    auth_mode,
                    None,
                    None,
                ));
            }
            Err(error) => {
                return Ok(result(
                    GithubSetupStatus::GhMissing,
                    Some("gh CLI not found on PATH; install GitHub CLI".to_owned()),
                    "unavailable",
                    false,
                    auth_mode,
                    None,
                    Some(error.code()),
                ));
            }
        };

        let auth_status = match self.command.auth_status(token).await {
            Ok(auth_status) => auth_status,
            Err(error) => {
                return Ok(result(
                    GithubSetupStatus::NotAuthenticated,
                    Some(
                        "gh is not authenticated; run `gh auth login` or configure a token"
                            .to_owned(),
                    ),
                    version.trim(),
                    false,
                    auth_mode,
                    None,
                    Some(error.code()),
                ));
            }
        };

        if !auth_status.authenticated {
            return Ok(result(
                GithubSetupStatus::NotAuthenticated,
                Some(
                    "gh is not authenticated; run `gh auth login` or configure a token".to_owned(),
                ),
                version.trim(),
                false,
                auth_mode,
                auth_status.account.as_deref(),
                None,
            ));
        }

        Ok(result(
            GithubSetupStatus::Ok,
            Some("GitHub CLI is installed and authenticated".to_owned()),
            version.trim(),
            true,
            auth_mode,
            auth_status.account.as_deref(),
            None,
        ))
    }
}

fn result(
    status: GithubSetupStatus,
    message: Option<String>,
    gh_version: &str,
    gh_authenticated: bool,
    auth_mode: &str,
    account: Option<&str>,
    setup_error_code: Option<&str>,
) -> GithubSetupCheckResult {
    let mut details = json!({
        "mode": "read",
        "fake_mode": false,
        "gh_version": gh_version,
        "gh_authenticated": gh_authenticated,
        "auth_mode": auth_mode,
        "status": status.as_str(),
    });
    if let Some(account) = account {
        details["gh_account"] = json!(account);
    }
    if let Some(code) = setup_error_code {
        details["setup_error_code"] = json!(code);
    }
    GithubSetupCheckResult {
        status,
        ok: status == GithubSetupStatus::Ok,
        message,
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{FakeGhCommand, GhAuthStatus, GhCommandError};

    #[tokio::test]
    async fn setup_check_reports_missing_gh_binary() {
        let gh = FakeGhCommand::default();
        gh.set_version_error(GhCommandError::Spawn {
            summary: "gh not found".to_owned(),
        });

        let result = GithubSetupCheck::new(gh)
            .run(None)
            .await
            .expect("setup check");

        assert_eq!(result.status, GithubSetupStatus::GhMissing);
        assert!(!result.ok);
        assert_eq!(
            result.message.as_deref(),
            Some("gh CLI not found on PATH; install GitHub CLI")
        );
        assert_eq!(result.details["status"], "gh_missing");
        assert_eq!(result.details["gh_authenticated"], false);
    }

    #[tokio::test]
    async fn setup_check_reports_unauthenticated_gh() {
        let gh = FakeGhCommand::default();
        gh.set_auth_status(GhAuthStatus {
            authenticated: false,
            account: None,
        });

        let result = GithubSetupCheck::new(gh)
            .run(None)
            .await
            .expect("setup check");

        assert_eq!(result.status, GithubSetupStatus::NotAuthenticated);
        assert!(!result.ok);
        assert_eq!(
            result.message.as_deref(),
            Some("gh is not authenticated; run `gh auth login` or configure a token")
        );
        assert_eq!(result.details["status"], "not_authenticated");
        assert_eq!(result.details["gh_authenticated"], false);
        assert_eq!(result.details["auth_mode"], "host");
    }

    #[tokio::test]
    async fn setup_check_reports_ready_gh() {
        let gh = FakeGhCommand::default();
        gh.set_auth_status(GhAuthStatus {
            authenticated: true,
            account: Some("fake-otto".to_owned()),
        });

        let result = GithubSetupCheck::new(gh)
            .run(None)
            .await
            .expect("setup check");

        assert_eq!(result.status, GithubSetupStatus::Ok);
        assert!(result.ok);
        assert_eq!(result.details["status"], "ok");
        assert_eq!(result.details["fake_mode"], false);
        assert_eq!(result.details["gh_authenticated"], true);
        assert_eq!(result.details["gh_account"], "fake-otto");
        assert!(
            result.details["gh_version"]
                .as_str()
                .is_some_and(|version| { version.starts_with("gh version 2.72.0") })
        );
    }
}
