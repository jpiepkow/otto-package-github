//! First-party GitHub tool package constants and registration helpers.

pub mod command;
pub mod setup_check;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use command::{GhCommand, GhCommandError};
use otto_extension_sdk::extension_ids::{
    CapabilityId, RoleId, SchemaId, SetupCheckId, ToolId, UiFormId,
};
use otto_extension_sdk::grants::CapabilityMode;
use otto_extension_sdk::protocol::{ToolInvokeParams, ToolInvokeResult};
use otto_extension_sdk::roles::{
    CapabilityDeclaration, ExtensionRegistrations, ExtensionRoleKind, RoleRegistration,
    SchemaRegistration, SetupCheckRegistration, ToolRegistration, UiFormRegistration,
};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const PACKAGE_ID: &str = "com.otto.github";
pub const DISPLAY_NAME: &str = "Default GitHub Tools";
pub const FAKE_MODE_ARG: &str = "--fake";
pub const ROLE_ID: &str = "role.default.tool.github";
pub const CAPABILITY_READ: &str = "cap.default.github.read";
pub const CAPABILITY_WRITE: &str = "cap.default.github.write";
pub const SETUP_READY: &str = "setup.default.github.ready";
pub const TOOL_SEARCH_CODE: &str = "tool.default.github.search_code";
pub const TOOL_FETCH_FILE: &str = "tool.default.github.fetch_file";
pub const TOOL_LIST_COMMITS: &str = "tool.default.github.list_commits";
pub const TOOL_LIST_PULL_REQUESTS: &str = "tool.default.github.list_pull_requests";
pub const TOOL_LIST_ISSUES: &str = "tool.default.github.list_issues";
pub const TOOL_PREPARE_CHECKOUT: &str = "tool.default.github.prepare_checkout";
pub const TOOL_GIT_STAGE: &str = "tool.default.github.git_stage";
pub const TOOL_GIT_COMMIT: &str = "tool.default.github.git_commit";
pub const TOOL_BRANCH_CREATE: &str = "tool.default.github.branch_create";
pub const TOOL_GIT_PUSH: &str = "tool.default.github.git_push";
pub const TOOL_PULL_REQUEST_OPEN: &str = "tool.default.github.pull_request_open";
pub const TOOL_PULL_REQUEST_MERGE: &str = "tool.default.github.pull_request_merge";
pub const TOOL_COMMENT: &str = "tool.default.github.comment";
pub const TOOL_ISSUE_CREATE: &str = "tool.default.github.issue_create";
pub const TOOL_LABEL_EDIT: &str = "tool.default.github.label_edit";
pub const TOOL_BRANCH_DELETE: &str = "tool.default.github.branch_delete";

pub const SCHEMA_SETUP_DETAILS: &str = "schema.default.github.setup_details";
pub const SCHEMA_GRANT_SCOPE: &str = "schema.default.github.grant_scope";
pub const SCHEMA_READ_OUTPUT: &str = "schema.default.github.read_output";
pub const SCHEMA_SEARCH_CODE_INPUT: &str = "schema.default.github.search_code_input";
pub const SCHEMA_FETCH_FILE_INPUT: &str = "schema.default.github.fetch_file_input";
pub const SCHEMA_LIST_COMMITS_INPUT: &str = "schema.default.github.list_commits_input";
pub const SCHEMA_LIST_PULL_REQUESTS_INPUT: &str = "schema.default.github.list_pull_requests_input";
pub const SCHEMA_LIST_ISSUES_INPUT: &str = "schema.default.github.list_issues_input";
pub const SCHEMA_PREPARE_CHECKOUT_INPUT: &str = "schema.default.github.prepare_checkout_input";
pub const SCHEMA_PREPARE_CHECKOUT_OUTPUT: &str = "schema.default.github.prepare_checkout_output";
pub const SCHEMA_GIT_STAGE_INPUT: &str = "schema.default.github.git_stage_input";
pub const SCHEMA_GIT_COMMIT_INPUT: &str = "schema.default.github.git_commit_input";
pub const SCHEMA_BRANCH_CREATE_INPUT: &str = "schema.default.github.branch_create_input";
pub const SCHEMA_GIT_PUSH_INPUT: &str = "schema.default.github.git_push_input";
pub const SCHEMA_PULL_REQUEST_OPEN_INPUT: &str = "schema.default.github.pull_request_open_input";
pub const SCHEMA_PULL_REQUEST_MERGE_INPUT: &str = "schema.default.github.pull_request_merge_input";
pub const SCHEMA_COMMENT_INPUT: &str = "schema.default.github.comment_input";
pub const SCHEMA_ISSUE_CREATE_INPUT: &str = "schema.default.github.issue_create_input";
pub const SCHEMA_LABEL_EDIT_INPUT: &str = "schema.default.github.label_edit_input";
pub const SCHEMA_BRANCH_DELETE_INPUT: &str = "schema.default.github.branch_delete_input";
pub const SCHEMA_WRITE_OUTPUT: &str = "schema.default.github.write_output";
pub const SCHEMA_SETUP_FORM: &str = "schema.default.github.setup_form";
pub const SCHEMA_GRANT_FORM: &str = "schema.default.github.grant_form";
pub const UI_SETUP: &str = "github_setup";
pub const UI_GRANT: &str = "github_grant";
const CHECKOUT_MOUNT_ROOT: &str = "/otto/checkout";
const CHECKOUT_FILE_SOFT_CAP: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRuntimeError {
    Validation { reason: String },
    External { summary: String },
}

impl ToolRuntimeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "validation_error",
            Self::External { .. } => "external_error",
        }
    }
}

impl fmt::Display for ToolRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation { reason } => write!(formatter, "validation error: {reason}"),
            Self::External { summary } => write!(formatter, "external error: {summary}"),
        }
    }
}

impl Error for ToolRuntimeError {}

impl From<GhCommandError> for ToolRuntimeError {
    fn from(error: GhCommandError) -> Self {
        Self::External {
            summary: error.to_string(),
        }
    }
}

pub type ToolRuntimeResult<T> = Result<T, ToolRuntimeError>;

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn registrations() -> ExtensionRegistrations {
    let read_capability = capability_id(CAPABILITY_READ);
    let write_capability = capability_id(CAPABILITY_WRITE);

    ExtensionRegistrations {
        roles: vec![RoleRegistration {
            id: role_id(ROLE_ID),
            kind: ExtensionRoleKind::ToolPackage,
            display_name: "Default GitHub tool package".to_owned(),
            capabilities: vec![read_capability.clone(), write_capability.clone()],
        }],
        schemas: vec![
            schema(
                SCHEMA_SETUP_DETAILS,
                "schemas/setup-details.schema.json",
                "GitHub setup-check details",
            ),
            schema(
                SCHEMA_GRANT_SCOPE,
                "schemas/grant-scope.schema.json",
                "GitHub tool grant scope",
            ),
            schema(
                SCHEMA_READ_OUTPUT,
                "schemas/read-output.schema.json",
                "Bounded GitHub read output",
            ),
            schema(
                SCHEMA_SEARCH_CODE_INPUT,
                "schemas/search-code-input.schema.json",
                "Search code input",
            ),
            schema(
                SCHEMA_FETCH_FILE_INPUT,
                "schemas/fetch-file-input.schema.json",
                "Fetch file input",
            ),
            schema(
                SCHEMA_LIST_COMMITS_INPUT,
                "schemas/list-commits-input.schema.json",
                "List commits input",
            ),
            schema(
                SCHEMA_LIST_PULL_REQUESTS_INPUT,
                "schemas/list-pull-requests-input.schema.json",
                "List pull requests input",
            ),
            schema(
                SCHEMA_LIST_ISSUES_INPUT,
                "schemas/list-issues-input.schema.json",
                "List issues input",
            ),
            schema(
                SCHEMA_PREPARE_CHECKOUT_INPUT,
                "schemas/prepare-checkout-input.schema.json",
                "Prepare checkout input",
            ),
            schema(
                SCHEMA_PREPARE_CHECKOUT_OUTPUT,
                "schemas/prepare-checkout-output.schema.json",
                "Prepare checkout output",
            ),
            schema(
                SCHEMA_GIT_STAGE_INPUT,
                "schemas/git-stage-input.schema.json",
                "Stage Git changes input",
            ),
            schema(
                SCHEMA_GIT_COMMIT_INPUT,
                "schemas/git-commit-input.schema.json",
                "Commit Git changes input",
            ),
            schema(
                SCHEMA_BRANCH_CREATE_INPUT,
                "schemas/branch-create-input.schema.json",
                "Create Git branch input",
            ),
            schema(
                SCHEMA_GIT_PUSH_INPUT,
                "schemas/git-push-input.schema.json",
                "Push Git ref input",
            ),
            schema(
                SCHEMA_PULL_REQUEST_OPEN_INPUT,
                "schemas/pull-request-open-input.schema.json",
                "Open pull request input",
            ),
            schema(
                SCHEMA_PULL_REQUEST_MERGE_INPUT,
                "schemas/pull-request-merge-input.schema.json",
                "Merge pull request input",
            ),
            schema(
                SCHEMA_COMMENT_INPUT,
                "schemas/comment-input.schema.json",
                "Comment input",
            ),
            schema(
                SCHEMA_ISSUE_CREATE_INPUT,
                "schemas/issue-create-input.schema.json",
                "Create issue input",
            ),
            schema(
                SCHEMA_LABEL_EDIT_INPUT,
                "schemas/label-edit-input.schema.json",
                "Edit label input",
            ),
            schema(
                SCHEMA_BRANCH_DELETE_INPUT,
                "schemas/branch-delete-input.schema.json",
                "Delete branch input",
            ),
            schema(
                SCHEMA_WRITE_OUTPUT,
                "schemas/write-output.schema.json",
                "GitHub write-result output",
            ),
            schema(
                SCHEMA_SETUP_FORM,
                "ui/setup.form.json",
                "GitHub setup form schema",
            ),
            schema(
                SCHEMA_GRANT_FORM,
                "ui/grant.form.json",
                "GitHub grant form schema",
            ),
        ],
        tools: vec![
            github_tool(
                TOOL_SEARCH_CODE,
                "Search GitHub code",
                "Search GitHub code through the host gh CLI. Use this for cross-repository discovery before fetching a specific file or preparing a checkout.",
                SCHEMA_SEARCH_CODE_INPUT,
                Some(SCHEMA_READ_OUTPUT),
                read_capability.clone(),
                false,
                Some(json!({"max_matches": 20, "context_lines": 3, "max_file_bytes": 32768})),
            ),
            github_tool(
                TOOL_FETCH_FILE,
                "Fetch GitHub file",
                "Fetch a bounded file slice from GitHub at a repository ref through the API. Use this when the path is known and a full checkout is unnecessary.",
                SCHEMA_FETCH_FILE_INPUT,
                Some(SCHEMA_READ_OUTPUT),
                read_capability.clone(),
                false,
                Some(json!({"max_lines": 120, "max_bytes": 32768})),
            ),
            github_tool(
                TOOL_LIST_COMMITS,
                "List GitHub commits",
                "List recent commits for a granted repository and ref. Use this to understand recent changes before investigating files or preparing a checkout.",
                SCHEMA_LIST_COMMITS_INPUT,
                Some(SCHEMA_READ_OUTPUT),
                read_capability.clone(),
                false,
                Some(json!({"max_results": 30})),
            ),
            github_tool(
                TOOL_LIST_PULL_REQUESTS,
                "List GitHub pull requests",
                "List pull requests for a granted repository. Use this to inspect active work, branch names, and review state before opening or commenting on a PR.",
                SCHEMA_LIST_PULL_REQUESTS_INPUT,
                Some(SCHEMA_READ_OUTPUT),
                read_capability.clone(),
                false,
                Some(json!({"max_results": 30})),
            ),
            github_tool(
                TOOL_LIST_ISSUES,
                "List GitHub issues",
                "List issues for a granted repository. Use this to find existing incident reports, bugs, or tasks before creating a new issue.",
                SCHEMA_LIST_ISSUES_INPUT,
                Some(SCHEMA_READ_OUTPUT),
                read_capability.clone(),
                false,
                Some(json!({"max_results": 30})),
            ),
            github_tool(
                TOOL_PREPARE_CHECKOUT,
                "Prepare GitHub checkout",
                "Prepare a host-owned repository checkout for a granted repository and ref, returning the container mount path for agent file investigation.",
                SCHEMA_PREPARE_CHECKOUT_INPUT,
                Some(SCHEMA_PREPARE_CHECKOUT_OUTPUT),
                read_capability.clone(),
                false,
                Some(json!({"max_clone_bytes": 524288000})),
            ),
            github_tool(
                TOOL_GIT_STAGE,
                "Stage Git changes",
                "Stage selected paths in a prepared checkout. This mutating git operation requires Otto approval before execution.",
                SCHEMA_GIT_STAGE_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_GIT_COMMIT,
                "Commit Git changes",
                "Create a commit in a prepared checkout with a supplied message. This mutating git operation requires Otto approval before execution.",
                SCHEMA_GIT_COMMIT_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_BRANCH_CREATE,
                "Create Git branch",
                "Create a local branch in a prepared checkout. This mutating git operation requires Otto approval before execution.",
                SCHEMA_BRANCH_CREATE_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_GIT_PUSH,
                "Push Git branch",
                "Push a refspec to GitHub, including force pushes when explicitly requested. This remote mutation requires Otto approval before execution.",
                SCHEMA_GIT_PUSH_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_PULL_REQUEST_OPEN,
                "Open GitHub pull request",
                "Open a pull request on GitHub from an approved branch and base. This remote mutation requires Otto approval before execution.",
                SCHEMA_PULL_REQUEST_OPEN_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_PULL_REQUEST_MERGE,
                "Merge GitHub pull request",
                "Merge an approved pull request, including protected-branch merges when GitHub permits them. This destructive remote mutation requires Otto approval before execution.",
                SCHEMA_PULL_REQUEST_MERGE_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_COMMENT,
                "Comment on GitHub",
                "Add a comment to a GitHub issue or pull request. This remote write requires Otto approval before execution.",
                SCHEMA_COMMENT_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_ISSUE_CREATE,
                "Create GitHub issue",
                "Create a GitHub issue in a granted repository. This remote write requires Otto approval before execution.",
                SCHEMA_ISSUE_CREATE_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_LABEL_EDIT,
                "Edit GitHub label",
                "Create or update a GitHub label in a granted repository. This remote mutation requires Otto approval before execution.",
                SCHEMA_LABEL_EDIT_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
            github_tool(
                TOOL_BRANCH_DELETE,
                "Delete GitHub branch",
                "Delete a branch reference on GitHub. This destructive remote mutation requires Otto approval before execution.",
                SCHEMA_BRANCH_DELETE_INPUT,
                Some(SCHEMA_WRITE_OUTPUT),
                write_capability.clone(),
                true,
                None,
            ),
        ],
        triggers: Vec::new(),
        setup_checks: vec![SetupCheckRegistration {
            id: setup_check_id(SETUP_READY),
            display_name: "GitHub tools ready".to_owned(),
            output_schema: Some(schema_id(SCHEMA_SETUP_DETAILS)),
            required_capabilities: vec![read_capability.clone()],
        }],
        ui_forms: vec![
            UiFormRegistration {
                id: ui_form_id(UI_SETUP),
                display_name: "GitHub setup".to_owned(),
                schema: schema_id(SCHEMA_SETUP_FORM),
            },
            UiFormRegistration {
                id: ui_form_id(UI_GRANT),
                display_name: "GitHub grant".to_owned(),
                schema: schema_id(SCHEMA_GRANT_FORM),
            },
        ],
        migrations: Vec::new(),
        redaction: Vec::new(),
        capabilities: vec![
            CapabilityDeclaration {
                id: read_capability,
                mode: CapabilityMode::Read,
                description: "Read scoped GitHub code search, file contents, commits, pull requests, issues, and checkout preparation.".to_owned(),
            },
            CapabilityDeclaration {
                id: write_capability,
                mode: CapabilityMode::Send,
                description: "Author explicitly approved GitHub changes through host-side git and gh operations.".to_owned(),
            },
        ],
    }
}

#[must_use]
pub const fn read_tool_ids() -> [&'static str; 6] {
    [
        TOOL_SEARCH_CODE,
        TOOL_FETCH_FILE,
        TOOL_LIST_COMMITS,
        TOOL_LIST_PULL_REQUESTS,
        TOOL_LIST_ISSUES,
        TOOL_PREPARE_CHECKOUT,
    ]
}

#[must_use]
pub const fn write_tool_ids() -> [&'static str; 10] {
    [
        TOOL_GIT_STAGE,
        TOOL_GIT_COMMIT,
        TOOL_BRANCH_CREATE,
        TOOL_GIT_PUSH,
        TOOL_PULL_REQUEST_OPEN,
        TOOL_PULL_REQUEST_MERGE,
        TOOL_COMMENT,
        TOOL_ISSUE_CREATE,
        TOOL_LABEL_EDIT,
        TOOL_BRANCH_DELETE,
    ]
}

#[must_use]
pub const fn tool_ids() -> [&'static str; 16] {
    [
        TOOL_SEARCH_CODE,
        TOOL_FETCH_FILE,
        TOOL_LIST_COMMITS,
        TOOL_LIST_PULL_REQUESTS,
        TOOL_LIST_ISSUES,
        TOOL_PREPARE_CHECKOUT,
        TOOL_GIT_STAGE,
        TOOL_GIT_COMMIT,
        TOOL_BRANCH_CREATE,
        TOOL_GIT_PUSH,
        TOOL_PULL_REQUEST_OPEN,
        TOOL_PULL_REQUEST_MERGE,
        TOOL_COMMENT,
        TOOL_ISSUE_CREATE,
        TOOL_LABEL_EDIT,
        TOOL_BRANCH_DELETE,
    ]
}

pub async fn invoke_fake_tool(
    params: &ToolInvokeParams,
) -> Result<ToolInvokeResult, ToolRuntimeError> {
    let gh = command::FakeGhCommand::default();
    invoke_tool_with_gh(&gh, params, None).await
}

pub async fn invoke_tool_with_gh<G>(
    gh: &G,
    params: &ToolInvokeParams,
    token: Option<&SecretString>,
) -> Result<ToolInvokeResult, ToolRuntimeError>
where
    G: GhCommand,
{
    invoke_tool_with_gh_inner(gh, params, token, None).await
}

async fn invoke_tool_with_gh_inner<G>(
    gh: &G,
    params: &ToolInvokeParams,
    token: Option<&SecretString>,
    otto_root_override: Option<&Path>,
) -> Result<ToolInvokeResult, ToolRuntimeError>
where
    G: GhCommand,
{
    match params.tool_id.as_str() {
        TOOL_SEARCH_CODE => {
            let scope = validate_read_scope(params)?;
            invoke_search_code(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_FETCH_FILE => {
            let scope = validate_read_scope(params)?;
            invoke_fetch_file(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_LIST_COMMITS => {
            let scope = validate_read_scope(params)?;
            invoke_list_commits(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_LIST_PULL_REQUESTS => {
            let scope = validate_read_scope(params)?;
            invoke_list_pull_requests(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_LIST_ISSUES => {
            let scope = validate_read_scope(params)?;
            invoke_list_issues(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_PREPARE_CHECKOUT => {
            let scope = validate_read_scope(params)?;
            let otto_root = otto_root_override
                .map(Path::to_path_buf)
                .map(Ok)
                .unwrap_or_else(otto_root_from_env)?;
            invoke_prepare_checkout(
                gh,
                &scope,
                &params.run_id.to_string(),
                &otto_root,
                params.arguments.clone(),
                token,
            )
            .await
        }
        TOOL_GIT_STAGE => {
            let scope = validate_write_scope(params)?;
            let otto_root = otto_root_override
                .map(Path::to_path_buf)
                .map(Ok)
                .unwrap_or_else(otto_root_from_env)?;
            invoke_git_stage(
                gh,
                &scope,
                &params.run_id.to_string(),
                &otto_root,
                params.arguments.clone(),
            )
            .await
        }
        TOOL_GIT_COMMIT => {
            let scope = validate_write_scope(params)?;
            let otto_root = otto_root_override
                .map(Path::to_path_buf)
                .map(Ok)
                .unwrap_or_else(otto_root_from_env)?;
            invoke_git_commit(
                gh,
                &scope,
                &params.run_id.to_string(),
                &otto_root,
                params.arguments.clone(),
            )
            .await
        }
        TOOL_BRANCH_CREATE => {
            let scope = validate_write_scope(params)?;
            let otto_root = otto_root_override
                .map(Path::to_path_buf)
                .map(Ok)
                .unwrap_or_else(otto_root_from_env)?;
            invoke_branch_create(
                gh,
                &scope,
                &params.run_id.to_string(),
                &otto_root,
                params.arguments.clone(),
            )
            .await
        }
        TOOL_GIT_PUSH => {
            let scope = validate_write_scope(params)?;
            let otto_root = otto_root_override
                .map(Path::to_path_buf)
                .map(Ok)
                .unwrap_or_else(otto_root_from_env)?;
            invoke_git_push(
                gh,
                &scope,
                &params.run_id.to_string(),
                &otto_root,
                params.arguments.clone(),
                token,
            )
            .await
        }
        TOOL_PULL_REQUEST_OPEN => {
            let scope = validate_write_scope(params)?;
            invoke_pull_request_open(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_PULL_REQUEST_MERGE => {
            let scope = validate_write_scope(params)?;
            invoke_pull_request_merge(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_COMMENT => {
            let scope = validate_write_scope(params)?;
            invoke_comment(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_ISSUE_CREATE => {
            let scope = validate_write_scope(params)?;
            invoke_issue_create(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_LABEL_EDIT => {
            let scope = validate_write_scope(params)?;
            invoke_label_edit(gh, &scope, params.arguments.clone(), token).await
        }
        TOOL_BRANCH_DELETE => {
            let scope = validate_write_scope(params)?;
            invoke_branch_delete(gh, &scope, params.arguments.clone(), token).await
        }
        _ => Err(ToolRuntimeError::Validation {
            reason: "unknown GitHub tool".to_owned(),
        }),
    }
}

async fn invoke_fetch_file<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<FetchFileArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!(
        "/repos/{owner}/{repo_name}/contents/{}?ref={}",
        args.path, args.ref_name
    );
    let response = gh.api_get(&endpoint, token).await?;
    let content = response
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolRuntimeError::External {
            summary: "GitHub contents response omitted content".to_owned(),
        })?
        .replace(['\n', '\r'], "");
    let bytes = STANDARD
        .decode(content)
        .map_err(|error| ToolRuntimeError::External {
            summary: format!("GitHub contents base64 decode failed: {error}"),
        })?;
    let text = String::from_utf8_lossy(&bytes);
    let all_lines = text.lines().collect::<Vec<_>>();
    let total_lines = all_lines.len();
    let selected = line_slice(&all_lines, args.line_start, args.line_end);
    let (mut lines, truncated) =
        bounded_lines(&selected, scope.max_file_lines, scope.max_file_bytes);
    if truncated {
        lines.push(format!(
            "[truncated: returned {} of {} selected lines; total_lines={total_lines}]",
            lines.len(),
            selected.len()
        ));
    }
    Ok(ok_result(
        "Fetched bounded GitHub file.",
        read_output(
            "Fetched bounded GitHub file.",
            lines,
            truncated,
            total_lines,
            json!({
                "repo": args.repo,
                "path": args.path,
                "ref": args.ref_name
            }),
        ),
    ))
}

async fn invoke_search_code<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<SearchCodeArgs>(arguments)?;
    let query = search_query_with_allowlist(&args, scope)?;
    let limit = scope.max_matches.saturating_add(1);
    let response = gh.search_code(&query, token, limit).await?;
    let matches = response
        .as_array()
        .ok_or_else(|| ToolRuntimeError::External {
            summary: "GitHub code search response was not an array".to_owned(),
        })?;
    let match_count = matches.len().min(scope.max_matches);
    let remainder = matches.len().saturating_sub(scope.max_matches);
    let mut rendered = Vec::new();
    for item in matches.iter().take(scope.max_matches) {
        let path = item
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let repo = item
            .get("repository")
            .and_then(|repository| repository.get("fullName"))
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        rendered.push(format!("{repo} {path}"));
        let Some(fragment) = item
            .get("textMatches")
            .and_then(Value::as_array)
            .and_then(|matches| matches.first())
            .and_then(|text_match| text_match.get("fragment"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        rendered.extend(
            fragment
                .lines()
                .take(scope.context_lines)
                .map(|line| format!("  {line}")),
        );
    }
    let rendered_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let (mut lines, truncated) =
        bounded_lines(&rendered_refs, scope.max_file_lines, scope.max_file_bytes);
    if truncated {
        lines.push(format!(
            "[truncated: returned {} of {} rendered lines]",
            lines.len(),
            rendered.len()
        ));
    }
    Ok(ok_result(
        "Searched bounded GitHub code.",
        read_output(
            "Searched bounded GitHub code.",
            lines,
            truncated,
            rendered.len(),
            json!({
                "match_count": match_count,
                "remainder": remainder
            }),
        ),
    ))
}

async fn invoke_list_commits<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<ListCommitsArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let max_results = capped_max_results(args.max_results, scope);
    let endpoint = format!(
        "/repos/{owner}/{repo_name}/commits?sha={}&per_page={max_results}",
        args.ref_name
    );
    let response = gh.api_get(&endpoint, token).await?;
    let items = response
        .as_array()
        .ok_or_else(|| ToolRuntimeError::External {
            summary: "GitHub commits response was not an array".to_owned(),
        })?;
    let rendered = items.iter().map(render_commit).collect::<Vec<_>>();
    let lines = bounded_list_lines(&rendered, max_results, scope.max_file_bytes);
    Ok(ok_result(
        "Listed bounded GitHub commits.",
        read_output(
            "Listed bounded GitHub commits.",
            lines.0,
            lines.1,
            rendered.len(),
            json!({
                "repo": args.repo,
                "ref": args.ref_name
            }),
        ),
    ))
}

async fn invoke_list_pull_requests<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<ListPullRequestsArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    validate_state(&args.state)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let max_results = capped_max_results(args.max_results, scope);
    let endpoint = format!(
        "/repos/{owner}/{repo_name}/pulls?state={}&per_page={max_results}",
        args.state
    );
    let response = gh.api_get(&endpoint, token).await?;
    let items = response
        .as_array()
        .ok_or_else(|| ToolRuntimeError::External {
            summary: "GitHub pull requests response was not an array".to_owned(),
        })?;
    let rendered = items.iter().map(render_pull_request).collect::<Vec<_>>();
    let lines = bounded_list_lines(&rendered, max_results, scope.max_file_bytes);
    Ok(ok_result(
        "Listed bounded GitHub pull requests.",
        read_output(
            "Listed bounded GitHub pull requests.",
            lines.0,
            lines.1,
            rendered.len(),
            json!({
                "repo": args.repo,
                "state": args.state
            }),
        ),
    ))
}

async fn invoke_list_issues<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<ListIssuesArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    validate_state(&args.state)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let max_results = capped_max_results(args.max_results, scope);
    let endpoint = format!(
        "/repos/{owner}/{repo_name}/issues?state={}&per_page={max_results}",
        args.state
    );
    let response = gh.api_get(&endpoint, token).await?;
    let items = response
        .as_array()
        .ok_or_else(|| ToolRuntimeError::External {
            summary: "GitHub issues response was not an array".to_owned(),
        })?;
    let rendered = items
        .iter()
        .filter(|item| item.get("pull_request").is_none())
        .map(render_issue)
        .collect::<Vec<_>>();
    let lines = bounded_list_lines(&rendered, max_results, scope.max_file_bytes);
    Ok(ok_result(
        "Listed bounded GitHub issues.",
        read_output(
            "Listed bounded GitHub issues.",
            lines.0,
            lines.1,
            rendered.len(),
            json!({
                "repo": args.repo,
                "state": args.state
            }),
        ),
    ))
}

async fn invoke_prepare_checkout<G>(
    gh: &G,
    scope: &PackageScope,
    run_id: &str,
    otto_root: &Path,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<PrepareCheckoutArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    validate_ref_allowed(&args.ref_name, scope)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!("/repos/{owner}/{repo_name}");
    let size_kb = repo_size_kb(gh, &endpoint, token).await?;
    let estimated_size_bytes = size_kb.saturating_mul(1024);
    if estimated_size_bytes > scope.max_clone_bytes as u64 {
        return Err(ToolRuntimeError::Validation {
            reason: format!(
                "repo exceeds max_clone_bytes: estimated {estimated_size_bytes} bytes > {} bytes",
                scope.max_clone_bytes
            ),
        });
    }

    let host_path = checkout_host_path(otto_root, run_id, owner, repo_name);
    let mount_path = checkout_mount_path(owner, repo_name);
    if let Some(parent) = host_path.parent() {
        fs::create_dir_all(parent).map_err(io_runtime_error)?;
    }
    remove_existing_checkout(&host_path)?;

    let mut commit_sha = gh.clone_repo(&args.repo, &host_path, token).await?;
    if args.ref_name != "HEAD" {
        commit_sha = gh.checkout_ref(&host_path, &args.ref_name).await?;
    }
    let stats = checkout_stats(&host_path, CHECKOUT_FILE_SOFT_CAP)?;
    let truncated = stats.truncated || stats.size_bytes > scope.max_clone_bytes as u64;
    let summary = format!(
        "Checked out {}@{} to {} ({} files)",
        args.repo, args.ref_name, mount_path, stats.file_count
    );

    Ok(ToolInvokeResult {
        status: "ok".to_owned(),
        summary: summary.clone(),
        output: json!({
            "status": "ok",
            "summary": summary,
            "output": {
                "host_path": host_path.to_string_lossy(),
                "mount_path": mount_path,
                "owner": owner,
                "repo": repo_name,
                "ref": args.ref_name,
                "commit_sha": commit_sha,
                "file_count": stats.file_count,
                "size_bytes": stats.size_bytes,
                "truncated": truncated
            }
        }),
    })
}

async fn invoke_git_stage<G>(
    gh: &G,
    scope: &PackageScope,
    run_id: &str,
    otto_root: &Path,
    arguments: Value,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<GitStageArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.paths.is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "git_stage requires at least one path".to_owned(),
        });
    }
    let host_path = prepared_checkout_host_path(otto_root, run_id, &args.repo)?;
    gh.git_stage(&host_path, &args.paths).await?;
    Ok(ok_result(
        "Staged Git paths in prepared checkout.",
        write_output(
            "Staged Git paths in prepared checkout.",
            json!({
                "repo": args.repo,
                "paths": args.paths,
            }),
        ),
    ))
}

async fn invoke_git_commit<G>(
    gh: &G,
    scope: &PackageScope,
    run_id: &str,
    otto_root: &Path,
    arguments: Value,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<GitCommitArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.message.trim().is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "git_commit message must not be empty".to_owned(),
        });
    }
    let host_path = prepared_checkout_host_path(otto_root, run_id, &args.repo)?;
    let commit_sha = gh.git_commit(&host_path, &args.message).await?;
    Ok(ok_result(
        "Committed Git changes in prepared checkout.",
        write_output(
            "Committed Git changes in prepared checkout.",
            json!({
                "repo": args.repo,
                "commit_sha": commit_sha,
            }),
        ),
    ))
}

async fn invoke_branch_create<G>(
    gh: &G,
    scope: &PackageScope,
    run_id: &str,
    otto_root: &Path,
    arguments: Value,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<BranchCreateArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.branch.trim().is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "branch_create branch must not be empty".to_owned(),
        });
    }
    let host_path = prepared_checkout_host_path(otto_root, run_id, &args.repo)?;
    let commit_sha = gh
        .branch_create(&host_path, &args.branch, &args.from_ref)
        .await?;
    Ok(ok_result(
        "Created Git branch in prepared checkout.",
        write_output(
            "Created Git branch in prepared checkout.",
            json!({
                "repo": args.repo,
                "branch": args.branch,
                "from_ref": args.from_ref,
                "commit_sha": commit_sha,
            }),
        ),
    ))
}

async fn invoke_git_push<G>(
    gh: &G,
    scope: &PackageScope,
    run_id: &str,
    otto_root: &Path,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<GitPushArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.refspec.trim().is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "git_push refspec must not be empty".to_owned(),
        });
    }
    if args.remote.trim().is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "git_push remote must not be empty".to_owned(),
        });
    }
    let host_path = prepared_checkout_host_path(otto_root, run_id, &args.repo)?;
    gh.git_push(&host_path, &args.remote, &args.refspec, args.force, token)
        .await?;
    Ok(ok_result(
        "Pushed Git ref from prepared checkout.",
        write_output(
            "Pushed Git ref from prepared checkout.",
            json!({
                "repo": args.repo,
                "remote": args.remote,
                "refspec": args.refspec,
                "force": args.force,
            }),
        ),
    ))
}

async fn invoke_pull_request_open<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<PullRequestOpenArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!("/repos/{owner}/{repo_name}/pulls");
    let mut body = Map::new();
    body.insert("title".to_owned(), Value::String(args.title));
    body.insert("head".to_owned(), Value::String(args.head));
    body.insert("base".to_owned(), Value::String(args.base));
    if let Some(body_text) = args.body {
        body.insert("body".to_owned(), Value::String(body_text));
    }
    let response = gh.api_post(&endpoint, Value::Object(body), token).await?;
    let number = response_number(&response, "pull request")?;
    let html_url = response_html_url(&response);
    Ok(ok_result(
        "Opened GitHub pull request.",
        write_output(
            "Opened GitHub pull request.",
            json!({
                "repo": args.repo,
                "number": number,
                "html_url": html_url,
            }),
        ),
    ))
}

async fn invoke_pull_request_merge<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<PullRequestMergeArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    validate_merge_method(&args.method)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!("/repos/{owner}/{repo_name}/pulls/{}/merge", args.number);
    let response = gh
        .api_put(
            &endpoint,
            json!({
                "merge_method": args.method,
            }),
            token,
        )
        .await?;
    Ok(ok_result(
        "Merged GitHub pull request.",
        write_output(
            "Merged GitHub pull request.",
            json!({
                "repo": args.repo,
                "number": args.number,
                "merged": response.get("merged").and_then(Value::as_bool).unwrap_or(true),
                "sha": response.get("sha").and_then(Value::as_str).unwrap_or(""),
            }),
        ),
    ))
}

async fn invoke_comment<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<CommentArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.body.trim().is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "comment body must not be empty".to_owned(),
        });
    }
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!(
        "/repos/{owner}/{repo_name}/issues/{}/comments",
        args.issue_or_pr_number
    );
    let response = gh
        .api_post(&endpoint, json!({ "body": args.body }), token)
        .await?;
    Ok(ok_result(
        "Added GitHub issue or pull request comment.",
        write_output(
            "Added GitHub issue or pull request comment.",
            json!({
                "repo": args.repo,
                "issue_or_pr_number": args.issue_or_pr_number,
                "id": response.get("id").and_then(Value::as_u64).unwrap_or(0),
                "html_url": response_html_url(&response),
            }),
        ),
    ))
}

async fn invoke_issue_create<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<IssueCreateArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!("/repos/{owner}/{repo_name}/issues");
    let mut body = Map::new();
    body.insert("title".to_owned(), Value::String(args.title));
    if let Some(body_text) = args.body {
        body.insert("body".to_owned(), Value::String(body_text));
    }
    let response = gh.api_post(&endpoint, Value::Object(body), token).await?;
    let number = response_number(&response, "issue")?;
    Ok(ok_result(
        "Created GitHub issue.",
        write_output(
            "Created GitHub issue.",
            json!({
                "repo": args.repo,
                "number": number,
                "html_url": response_html_url(&response),
            }),
        ),
    ))
}

async fn invoke_label_edit<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<LabelEditArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.color.is_none() && args.description.is_none() {
        return Err(ToolRuntimeError::Validation {
            reason: "label_edit requires color or description".to_owned(),
        });
    }
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!("/repos/{owner}/{repo_name}/labels/{}", args.name);
    let mut body = Map::new();
    if let Some(color) = args.color {
        body.insert("color".to_owned(), Value::String(color));
    }
    if let Some(description) = args.description {
        body.insert("description".to_owned(), Value::String(description));
    }
    let response = gh.api_patch(&endpoint, Value::Object(body), token).await?;
    Ok(ok_result(
        "Edited GitHub label.",
        write_output(
            "Edited GitHub label.",
            json!({
                "repo": args.repo,
                "name": response.get("name").and_then(Value::as_str).unwrap_or(&args.name),
                "color": response.get("color").and_then(Value::as_str).unwrap_or(""),
            }),
        ),
    ))
}

async fn invoke_branch_delete<G>(
    gh: &G,
    scope: &PackageScope,
    arguments: Value,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<ToolInvokeResult>
where
    G: GhCommand,
{
    let args = decode_value::<BranchDeleteArgs>(arguments)?;
    validate_repo_allowed(&args.repo, scope)?;
    if args.branch.trim().is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "branch_delete branch must not be empty".to_owned(),
        });
    }
    let (owner, repo_name) = split_repo(&args.repo)?;
    let endpoint = format!("/repos/{owner}/{repo_name}/git/refs/heads/{}", args.branch);
    gh.api_delete(&endpoint, token).await?;
    Ok(ok_result(
        "Deleted GitHub branch reference.",
        write_output(
            "Deleted GitHub branch reference.",
            json!({
                "repo": args.repo,
                "branch": args.branch,
            }),
        ),
    ))
}

pub fn validate_read_scope(params: &ToolInvokeParams) -> ToolRuntimeResult<PackageScope> {
    if params.mode != CapabilityMode::Read {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub read tools require read mode".to_owned(),
        });
    }
    let scope = decode_scope(params)?;
    if scope.mode != "read" {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub package scope mode must be read".to_owned(),
        });
    }
    validate_common_scope(&scope)?;
    Ok(scope)
}

pub fn validate_write_scope(params: &ToolInvokeParams) -> ToolRuntimeResult<PackageScope> {
    if params.mode != CapabilityMode::Send {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub write tools require send mode".to_owned(),
        });
    }
    let scope = decode_scope(params)?;
    if scope.mode != "write" && scope.mode != "send" {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub package scope mode must be write".to_owned(),
        });
    }
    validate_common_scope(&scope)?;
    Ok(scope)
}

fn decode_scope(params: &ToolInvokeParams) -> ToolRuntimeResult<PackageScope> {
    let mut scope =
        serde_json::from_value::<PackageScope>(params.package_scope.clone()).map_err(|_| {
            ToolRuntimeError::Validation {
                reason: "GitHub package scope is invalid".to_owned(),
            }
        })?;
    scope.apply_default_bounds();
    Ok(scope)
}

fn validate_common_scope(scope: &PackageScope) -> ToolRuntimeResult<()> {
    if scope.auth_mode != "host" && scope.auth_mode != "token" {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub auth mode must be host or token".to_owned(),
        });
    }
    if scope.auth_mode == "token" && scope.credential_ref.as_deref().unwrap_or("").is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub token auth requires credential_ref".to_owned(),
        });
    }
    if !scope.unrestricted && scope.allowed_repos.is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub package scope requires at least one repository".to_owned(),
        });
    }
    if !scope.unrestricted && scope.allowed_refs.is_empty() {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub package scope requires at least one ref".to_owned(),
        });
    }
    if scope.max_file_lines == 0
        || scope.max_file_bytes == 0
        || scope.max_matches == 0
        || scope.max_results == 0
        || scope.max_clone_bytes == 0
    {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub scope bounds must be positive".to_owned(),
        });
    }
    Ok(())
}

pub fn validate_repo_allowed(repo: &str, scope: &PackageScope) -> ToolRuntimeResult<()> {
    if !scope.unrestricted
        && !scope.allowed_repos.is_empty()
        && !scope.allowed_repos.iter().any(|allowed| allowed == repo)
    {
        return Err(ToolRuntimeError::Validation {
            reason: format!("GitHub repository {repo} is outside package scope"),
        });
    }
    Ok(())
}

fn validate_ref_allowed(ref_name: &str, scope: &PackageScope) -> ToolRuntimeResult<()> {
    if scope.unrestricted
        || scope
            .allowed_refs
            .iter()
            .any(|allowed| allowed == ref_name || allowed == "*")
    {
        return Ok(());
    }
    Err(ToolRuntimeError::Validation {
        reason: format!("GitHub ref {ref_name} is outside package scope"),
    })
}

fn split_repo(repo: &str) -> ToolRuntimeResult<(&str, &str)> {
    let Some((owner, name)) = repo.split_once('/') else {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub repository must be owner/repo".to_owned(),
        });
    };
    if !safe_repo_segment(owner) || !safe_repo_segment(name) {
        return Err(ToolRuntimeError::Validation {
            reason: "GitHub repository must be owner/repo".to_owned(),
        });
    }
    Ok((owner, name))
}

fn safe_repo_segment(segment: &str) -> bool {
    !segment.is_empty() && segment != "." && segment != ".."
}

async fn repo_size_kb<G>(
    gh: &G,
    endpoint: &str,
    token: Option<&SecretString>,
) -> ToolRuntimeResult<u64>
where
    G: GhCommand,
{
    let response = gh.api_get(endpoint, token).await?;
    response
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolRuntimeError::External {
            summary: "GitHub repository response omitted size".to_owned(),
        })
}

fn checkout_host_path(otto_root: &Path, run_id: &str, owner: &str, repo_name: &str) -> PathBuf {
    otto_root
        .join("runs")
        .join(run_id)
        .join("github")
        .join("checkout")
        .join(owner)
        .join(repo_name)
}

fn checkout_mount_path(owner: &str, repo_name: &str) -> String {
    format!("{CHECKOUT_MOUNT_ROOT}/{owner}/{repo_name}")
}

fn prepared_checkout_host_path(
    otto_root: &Path,
    run_id: &str,
    repo: &str,
) -> ToolRuntimeResult<PathBuf> {
    let (owner, repo_name) = split_repo(repo)?;
    let host_path = checkout_host_path(otto_root, run_id, owner, repo_name);
    if !host_path.is_dir() {
        return Err(ToolRuntimeError::Validation {
            reason: format!(
                "prepared checkout for {repo} does not exist; run prepare_checkout first"
            ),
        });
    }
    Ok(host_path)
}

fn remove_existing_checkout(path: &Path) -> ToolRuntimeResult<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(io_runtime_error)
    } else {
        fs::remove_file(path).map_err(io_runtime_error)
    }
}

#[derive(Debug, Clone, Copy)]
struct CheckoutStats {
    file_count: usize,
    size_bytes: u64,
    truncated: bool,
}

fn checkout_stats(root: &Path, file_soft_cap: usize) -> ToolRuntimeResult<CheckoutStats> {
    let mut stack = vec![root.to_path_buf()];
    let mut file_count = 0usize;
    let mut size_bytes = 0u64;
    let mut truncated = false;

    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path).map_err(io_runtime_error)?;
        for entry in entries {
            let entry = entry.map_err(io_runtime_error)?;
            let metadata = entry.metadata().map_err(io_runtime_error)?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                if file_count >= file_soft_cap {
                    truncated = true;
                    continue;
                }
                file_count = file_count.saturating_add(1);
                size_bytes = size_bytes.saturating_add(metadata.len());
            }
        }
    }

    Ok(CheckoutStats {
        file_count,
        size_bytes,
        truncated,
    })
}

fn otto_root_from_env() -> ToolRuntimeResult<PathBuf> {
    let value = std::env::var_os("OTTO_ROOT")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolRuntimeError::Validation {
            reason: "OTTO_ROOT is required for prepare_checkout".to_owned(),
        })?;
    Ok(PathBuf::from(value))
}

fn io_runtime_error(error: std::io::Error) -> ToolRuntimeError {
    ToolRuntimeError::External {
        summary: error.to_string(),
    }
}

fn decode_value<T>(value: Value) -> ToolRuntimeResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value).map_err(|_| ToolRuntimeError::Validation {
        reason: "GitHub tool arguments are invalid".to_owned(),
    })
}

fn line_slice<'a>(
    lines: &'a [&'a str],
    line_start: Option<usize>,
    line_end: Option<usize>,
) -> Vec<&'a str> {
    let total = lines.len();
    let start = line_start.unwrap_or(1).max(1);
    let end = line_end.unwrap_or(total).min(total);
    if start > end || start > total {
        return Vec::new();
    }
    lines[start - 1..end].to_vec()
}

fn search_query_with_allowlist(
    args: &SearchCodeArgs,
    scope: &PackageScope,
) -> ToolRuntimeResult<String> {
    if let Some(repo) = &args.repo {
        validate_repo_allowed(repo, scope)?;
        return Ok(format!("{} repo:{repo}", args.query));
    }

    let repo_terms = scope
        .allowed_repos
        .iter()
        .map(|repo| format!("repo:{repo}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("{} {repo_terms}", args.query))
}

fn capped_max_results(requested: Option<usize>, scope: &PackageScope) -> usize {
    requested
        .unwrap_or(scope.max_results)
        .min(scope.max_results)
        .max(1)
}

fn validate_state(state: &str) -> ToolRuntimeResult<()> {
    if matches!(state, "open" | "closed" | "all") {
        return Ok(());
    }
    Err(ToolRuntimeError::Validation {
        reason: "GitHub state must be open, closed, or all".to_owned(),
    })
}

fn validate_merge_method(method: &str) -> ToolRuntimeResult<()> {
    if matches!(method, "merge" | "squash" | "rebase") {
        return Ok(());
    }
    Err(ToolRuntimeError::Validation {
        reason: "pull_request_merge method must be merge, squash, or rebase".to_owned(),
    })
}

fn response_number(response: &Value, name: &str) -> ToolRuntimeResult<u64> {
    response
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolRuntimeError::External {
            summary: format!("GitHub {name} response omitted number"),
        })
}

fn response_html_url(response: &Value) -> String {
    response
        .get("html_url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn render_commit(item: &Value) -> String {
    let sha = item
        .get("sha")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let short_sha = sha.chars().take(7).collect::<String>();
    let message = item
        .get("commit")
        .and_then(|commit| commit.get("message"))
        .and_then(Value::as_str)
        .and_then(|message| message.lines().next())
        .unwrap_or("<no message>");
    let author = item
        .get("commit")
        .and_then(|commit| commit.get("author"))
        .and_then(|author| author.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    format!("{short_sha}  {message}  ({author})")
}

fn render_pull_request(item: &Value) -> String {
    let number = item.get("number").and_then(Value::as_u64).unwrap_or(0);
    let state = item
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("<no title>");
    let head_ref = item
        .get("head")
        .and_then(|head| head.get("ref"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let base_ref = item
        .get("base")
        .and_then(|base| base.get("ref"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let user = item
        .get("user")
        .and_then(|user| user.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    format!("#{number} [{state}] {title}  {head_ref} -> {base_ref}  (@{user})")
}

fn render_issue(item: &Value) -> String {
    let number = item.get("number").and_then(Value::as_u64).unwrap_or(0);
    let state = item
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("<no title>");
    let user = item
        .get("user")
        .and_then(|user| user.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let labels = item
        .get("labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    format!("#{number} [{state}] {title}  (@{user})  labels: {labels}")
}

fn bounded_list_lines(
    rendered: &[String],
    max_results: usize,
    max_bytes: usize,
) -> (Vec<String>, bool) {
    let rendered_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let (mut lines, truncated) = bounded_lines(&rendered_refs, max_results, max_bytes);
    if truncated {
        lines.push(format!(
            "[truncated: returned {} of {} list rows]",
            lines.len(),
            rendered.len()
        ));
    }
    (lines, truncated)
}

fn ok_result(summary: &str, output: Value) -> ToolInvokeResult {
    ToolInvokeResult {
        status: "ok".to_owned(),
        summary: summary.to_owned(),
        output,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn read_output(
    summary: &str,
    lines: Vec<String>,
    truncated: bool,
    total_lines: usize,
    extra: Value,
) -> Value {
    let mut output = json!({
        "lines": lines,
        "truncated": truncated,
        "total_lines": total_lines
    });
    if let (Some(output), Some(extra)) = (output.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            output.insert(key.clone(), value.clone());
        }
    }
    json!({
        "status": "ok",
        "summary": summary,
        "output": output
    })
}

fn write_output(summary: &str, output: Value) -> Value {
    json!({
        "status": "ok",
        "summary": summary,
        "output": output
    })
}

pub fn bounded_lines(source: &[&str], max_lines: usize, max_bytes: usize) -> (Vec<String>, bool) {
    let mut output = Vec::new();
    let mut used_bytes = 0usize;
    let mut truncated = source.len() > max_lines;

    for line in source.iter().take(max_lines) {
        let separator = usize::from(!output.is_empty());
        let needed = separator + line.len();
        if used_bytes + needed <= max_bytes {
            output.push((*line).to_owned());
            used_bytes += needed;
            continue;
        }

        truncated = true;
        if output.is_empty() && max_bytes > 0 {
            output.push(truncate_ascii(line, max_bytes));
        }
        break;
    }

    (output, truncated)
}

pub fn truncate_ascii(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    if max_bytes <= 3 {
        return ".".repeat(max_bytes);
    }

    let mut output = value[..max_bytes - 3].to_owned();
    output.push_str("...");
    output
}

fn schema(id: &str, path: &str, description: &str) -> SchemaRegistration {
    SchemaRegistration {
        id: schema_id(id),
        path: path.to_owned(),
        description: Some(description.to_owned()),
    }
}

fn github_tool(
    id: &str,
    display_name: &str,
    description: &str,
    input_schema: &str,
    output_schema: Option<&str>,
    capability: CapabilityId,
    requires_approval: bool,
    scope_defaults: Option<Value>,
) -> ToolRegistration {
    ToolRegistration {
        id: tool_id(id),
        display_name: display_name.to_owned(),
        description: Some(description.to_owned()),
        input_schema: schema_id(input_schema),
        output_schema: output_schema.map(schema_id),
        required_capabilities: vec![capability],
        requires_approval: Some(requires_approval),
        runtime_commands: vec!["gh".to_owned(), "git".to_owned()],
        scope_defaults,
    }
}

fn role_id(value: &str) -> RoleId {
    RoleId::new(value).expect("valid role id")
}

fn tool_id(value: &str) -> ToolId {
    ToolId::new(value).expect("valid tool id")
}

fn schema_id(value: &str) -> SchemaId {
    SchemaId::new(value).expect("valid schema id")
}

fn setup_check_id(value: &str) -> SetupCheckId {
    SetupCheckId::new(value).expect("valid setup check id")
}

fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("valid capability id")
}

fn ui_form_id(value: &str) -> UiFormId {
    UiFormId::new(value).expect("valid UI form id")
}

const DEFAULT_MAX_FILE_LINES: usize = 120;
const DEFAULT_MAX_FILE_BYTES: usize = 32_768;
const DEFAULT_MAX_MATCHES: usize = 20;
const DEFAULT_MAX_RESULTS: usize = 30;
const DEFAULT_CONTEXT_LINES: usize = 3;
const DEFAULT_MAX_CLONE_BYTES: usize = 524_288_000;

#[derive(Debug, Deserialize)]
pub struct PackageScope {
    #[serde(default)]
    pub mode: String,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub unrestricted: bool,
    #[serde(default)]
    pub allowed_repos: Vec<String>,
    #[serde(default)]
    pub allowed_refs: Vec<String>,
    #[serde(default)]
    pub max_file_lines: usize,
    #[serde(default)]
    pub max_file_bytes: usize,
    #[serde(default)]
    pub max_matches: usize,
    #[serde(default)]
    pub max_results: usize,
    #[serde(default)]
    pub context_lines: usize,
    #[serde(default)]
    pub max_clone_bytes: usize,
    #[serde(default, flatten)]
    _extra: Map<String, Value>,
}

fn default_auth_mode() -> String {
    "host".to_owned()
}

impl PackageScope {
    fn apply_default_bounds(&mut self) {
        if self.max_file_lines == 0 {
            self.max_file_lines = DEFAULT_MAX_FILE_LINES;
        }
        if self.max_file_bytes == 0 {
            self.max_file_bytes = DEFAULT_MAX_FILE_BYTES;
        }
        if self.max_matches == 0 {
            self.max_matches = DEFAULT_MAX_MATCHES;
        }
        if self.max_results == 0 {
            self.max_results = DEFAULT_MAX_RESULTS;
        }
        if self.context_lines == 0 {
            self.context_lines = DEFAULT_CONTEXT_LINES;
        }
        if self.max_clone_bytes == 0 {
            self.max_clone_bytes = DEFAULT_MAX_CLONE_BYTES;
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchFileArgs {
    repo: String,
    path: String,
    #[serde(default = "default_ref", rename = "ref")]
    ref_name: String,
    #[serde(default)]
    line_start: Option<usize>,
    #[serde(default)]
    line_end: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchCodeArgs {
    query: String,
    #[serde(default)]
    repo: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListCommitsArgs {
    repo: String,
    #[serde(default = "default_ref", rename = "ref")]
    ref_name: String,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListPullRequestsArgs {
    repo: String,
    #[serde(default = "default_state")]
    state: String,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListIssuesArgs {
    repo: String,
    #[serde(default = "default_state")]
    state: String,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareCheckoutArgs {
    repo: String,
    #[serde(default = "default_ref", rename = "ref")]
    ref_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitStageArgs {
    repo: String,
    #[serde(default = "default_stage_paths")]
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitCommitArgs {
    repo: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BranchCreateArgs {
    repo: String,
    branch: String,
    #[serde(default = "default_ref")]
    from_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitPushArgs {
    repo: String,
    refspec: String,
    #[serde(default = "default_false")]
    force: bool,
    #[serde(default = "default_remote")]
    remote: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PullRequestOpenArgs {
    repo: String,
    title: String,
    head: String,
    base: String,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PullRequestMergeArgs {
    repo: String,
    number: u64,
    #[serde(default = "default_merge_method")]
    method: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentArgs {
    repo: String,
    issue_or_pr_number: u64,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IssueCreateArgs {
    repo: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelEditArgs {
    repo: String,
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BranchDeleteArgs {
    repo: String,
    branch: String,
}

fn default_ref() -> String {
    "HEAD".to_owned()
}

fn default_state() -> String {
    "open".to_owned()
}

fn default_stage_paths() -> Vec<String> {
    vec![".".to_owned()]
}

fn default_remote() -> String {
    "origin".to_owned()
}

fn default_merge_method() -> String {
    "merge".to_owned()
}

const fn default_false() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::FakeGhCommand;
    use base64::engine::general_purpose::STANDARD;
    use otto_extension_sdk::extension_ids::ToolId;
    use otto_extension_sdk::grants::CapabilityMode;
    use otto_extension_sdk::ids::{GrantId, RunId};
    use otto_extension_sdk::protocol::ToolInvokeParams;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn fetch_file_decodes_and_caps_large_file_output() {
        let gh = FakeGhCommand::default();
        let text = (1..=300)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let encoded = STANDARD.encode(text.as_bytes());
        gh.put_api_response(
            "api_get",
            "/repos/owner/repo/contents/src/lib.rs?ref=main",
            json!({
                "content": encoded,
                "encoding": "base64",
                "path": "src/lib.rs",
                "sha": "0123456789abcdef"
            }),
        );

        let result = invoke_tool_with_gh(
            &gh,
            &invoke_params(
                TOOL_FETCH_FILE,
                json!({
                    "repo": "owner/repo",
                    "path": "src/lib.rs",
                    "ref": "main"
                }),
            ),
            None,
        )
        .await
        .expect("fetch_file succeeds");

        assert_eq!(result.output["output"]["truncated"], true);
        assert_eq!(result.output["output"]["total_lines"], 300);
        let lines = result.output["output"]["lines"]
            .as_array()
            .expect("lines array");
        assert_eq!(lines.len(), 121);
        assert!(
            lines
                .iter()
                .any(|line| line.as_str().is_some_and(|line| line.contains("truncated")))
        );
    }

    #[tokio::test]
    async fn search_code_caps_matches_and_reports_remainder() {
        let gh = FakeGhCommand::default();
        let matches = (1..=25)
            .map(|index| {
                json!({
                    "path": format!("src/module_{index}.rs"),
                    "repository": { "fullName": "owner/repo" },
                    "textMatches": [
                        {
                            "fragment": format!("before {index}\nmatch {index}\nafter {index}")
                        }
                    ]
                })
            })
            .collect::<Vec<_>>();
        gh.put_response(
            "search_code:panic repo:owner/repo|21",
            Value::Array(matches),
        );

        let result = invoke_tool_with_gh(
            &gh,
            &invoke_params(
                TOOL_SEARCH_CODE,
                json!({
                    "query": "panic",
                    "repo": "owner/repo"
                }),
            ),
            None,
        )
        .await
        .expect("search_code succeeds");

        assert_eq!(result.output["output"]["match_count"], 20);
        assert_eq!(result.output["output"]["remainder"], 5);
        let lines = result.output["output"]["lines"]
            .as_array()
            .expect("lines array");
        assert!(lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("src/module_20.rs"))
        }));
        assert!(!lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("src/module_21.rs"))
        }));
    }

    #[tokio::test]
    async fn list_commits_caps_rendered_rows() {
        let gh = FakeGhCommand::default();
        let commits = (1..=50)
            .map(|index| {
                json!({
                    "sha": format!("{index:040}"),
                    "commit": {
                        "message": format!("Commit {index}\n\nBody"),
                        "author": { "name": format!("Author {index}") }
                    }
                })
            })
            .collect::<Vec<_>>();
        gh.put_api_response(
            "api_get",
            "/repos/owner/repo/commits?sha=main&per_page=30",
            Value::Array(commits),
        );

        let result = invoke_tool_with_gh(
            &gh,
            &invoke_params(
                TOOL_LIST_COMMITS,
                json!({
                    "repo": "owner/repo",
                    "ref": "main",
                    "max_results": 30
                }),
            ),
            None,
        )
        .await
        .expect("list_commits succeeds");

        assert_eq!(result.output["output"]["truncated"], true);
        let lines = result.output["output"]["lines"]
            .as_array()
            .expect("lines array");
        let rendered_rows = lines
            .iter()
            .filter(|line| {
                line.as_str()
                    .is_some_and(|line| !line.contains("truncated"))
            })
            .count();
        assert_eq!(rendered_rows, 30);
        assert!(
            lines
                .iter()
                .any(|line| line.as_str().is_some_and(|line| line.contains("Commit 30")))
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.as_str().is_some_and(|line| line.contains("Commit 31")))
        );
    }

    #[tokio::test]
    async fn list_pull_requests_renders_branch_context() {
        let gh = FakeGhCommand::default();
        gh.put_api_response(
            "api_get",
            "/repos/owner/repo/pulls?state=open&per_page=30",
            json!([
                {
                    "number": 7,
                    "state": "open",
                    "title": "Add checkout support",
                    "head": { "ref": "feature/github-checkout" },
                    "base": { "ref": "main" },
                    "user": { "login": "alice" }
                }
            ]),
        );

        let result = invoke_tool_with_gh(
            &gh,
            &invoke_params(
                TOOL_LIST_PULL_REQUESTS,
                json!({
                    "repo": "owner/repo",
                    "state": "open",
                    "max_results": 30
                }),
            ),
            None,
        )
        .await
        .expect("list_pull_requests succeeds");

        let lines = result.output["output"]["lines"]
            .as_array()
            .expect("lines array");
        assert!(lines.iter().any(|line| {
            line.as_str().is_some_and(|line| {
                line.contains("#7 [open] Add checkout support")
                    && line.contains("feature/github-checkout -> main")
                    && line.contains("@alice")
            })
        }));
    }

    #[tokio::test]
    async fn list_issues_filters_pull_request_entries() {
        let gh = FakeGhCommand::default();
        gh.put_api_response(
            "api_get",
            "/repos/owner/repo/issues?state=open&per_page=30",
            json!([
                {
                    "number": 11,
                    "state": "open",
                    "title": "Real issue",
                    "user": { "login": "bob" },
                    "labels": [{ "name": "bug" }, { "name": "github" }]
                },
                {
                    "number": 12,
                    "state": "open",
                    "title": "PR masquerading as issue",
                    "user": { "login": "carol" },
                    "labels": [],
                    "pull_request": { "url": "https://api.github.com/repos/owner/repo/pulls/12" }
                }
            ]),
        );

        let result = invoke_tool_with_gh(
            &gh,
            &invoke_params(
                TOOL_LIST_ISSUES,
                json!({
                    "repo": "owner/repo",
                    "state": "open",
                    "max_results": 30
                }),
            ),
            None,
        )
        .await
        .expect("list_issues succeeds");

        let lines = result.output["output"]["lines"]
            .as_array()
            .expect("lines array");
        assert!(lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("#11 [open] Real issue"))
        }));
        assert!(!lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("PR masquerading as issue"))
        }));
    }

    #[tokio::test]
    async fn prepare_checkout_returns_mount_path_contract() {
        let gh = FakeGhCommand::default();
        gh.put_api_response("api_get", "/repos/owner/repo", json!({ "size": 1 }));
        let otto_root = test_otto_root("mount-path");

        let result = invoke_tool_with_gh_inner(
            &gh,
            &invoke_params(
                TOOL_PREPARE_CHECKOUT,
                json!({
                    "repo": "owner/repo",
                    "ref": "main"
                }),
            ),
            None,
            Some(&otto_root),
        )
        .await
        .expect("prepare_checkout succeeds");

        assert_eq!(result.output["status"], "ok");
        assert_eq!(
            result.output["output"]["mount_path"],
            "/otto/checkout/owner/repo"
        );
        assert!(
            result.output["output"]["commit_sha"]
                .as_str()
                .is_some_and(|sha| !sha.is_empty())
        );
        assert_eq!(gh.clone_count(), 1);
        let _ = fs::remove_dir_all(otto_root);
    }

    #[tokio::test]
    async fn prepare_checkout_rejects_over_cap_before_clone() {
        let gh = FakeGhCommand::default();
        gh.put_api_response("api_get", "/repos/owner/repo", json!({ "size": 2 }));
        let mut params = invoke_params(
            TOOL_PREPARE_CHECKOUT,
            json!({
                "repo": "owner/repo",
                "ref": "main"
            }),
        );
        params.package_scope["max_clone_bytes"] = json!(1024);

        let error =
            invoke_tool_with_gh_inner(&gh, &params, None, Some(&test_otto_root("over-cap")))
                .await
                .expect_err("over-cap checkout fails");

        assert!(matches!(error, ToolRuntimeError::Validation { .. }));
        assert_eq!(gh.clone_count(), 0);
    }

    #[tokio::test]
    async fn git_commit_returns_commit_sha_from_prepared_checkout() {
        let gh = FakeGhCommand::default();
        let otto_root = test_otto_root("git-commit");
        let params = write_invoke_params(
            TOOL_GIT_COMMIT,
            json!({
                "repo": "owner/repo",
                "message": "Synthetic commit"
            }),
        );
        let host_path = checkout_host_path(&otto_root, &params.run_id.to_string(), "owner", "repo");
        fs::create_dir_all(&host_path).expect("create prepared checkout");

        let result = invoke_tool_with_gh_inner(&gh, &params, None, Some(&otto_root))
            .await
            .expect("git_commit succeeds");

        assert_eq!(
            result.output["output"]["commit_sha"],
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert!(
            gh.invocations()
                .iter()
                .any(|invocation| invocation.method == "git_commit")
        );
        let _ = fs::remove_dir_all(otto_root);
    }

    #[tokio::test]
    async fn git_push_force_records_force_flag_under_write_scope() {
        let gh = FakeGhCommand::default();
        let otto_root = test_otto_root("git-push-force");
        let params = write_invoke_params(
            TOOL_GIT_PUSH,
            json!({
                "repo": "owner/repo",
                "refspec": "HEAD:refs/heads/feature",
                "force": true
            }),
        );
        let host_path = checkout_host_path(&otto_root, &params.run_id.to_string(), "owner", "repo");
        fs::create_dir_all(&host_path).expect("create prepared checkout");

        let result = invoke_tool_with_gh_inner(&gh, &params, None, Some(&otto_root))
            .await
            .expect("git_push succeeds");

        assert_eq!(result.status, "ok");
        let invocations = gh.invocations();
        let push = invocations
            .iter()
            .find(|invocation| invocation.method == "git_push")
            .expect("git_push invocation recorded");
        assert_eq!(push.args[3], "true");
        let _ = fs::remove_dir_all(otto_root);
    }

    #[tokio::test]
    async fn local_git_write_tools_reject_read_scope() {
        let gh = FakeGhCommand::default();
        let cases = [
            (
                TOOL_GIT_STAGE,
                json!({
                    "repo": "owner/repo",
                    "paths": ["."]
                }),
            ),
            (
                TOOL_GIT_COMMIT,
                json!({
                    "repo": "owner/repo",
                    "message": "Synthetic commit"
                }),
            ),
            (
                TOOL_BRANCH_CREATE,
                json!({
                    "repo": "owner/repo",
                    "branch": "feature/synthetic"
                }),
            ),
            (
                TOOL_GIT_PUSH,
                json!({
                    "repo": "owner/repo",
                    "refspec": "HEAD:refs/heads/feature",
                    "force": true
                }),
            ),
        ];

        for (tool_id, arguments) in cases {
            let error = invoke_tool_with_gh(&gh, &invoke_params(tool_id, arguments), None)
                .await
                .expect_err("read grant rejects write tool");
            assert_eq!(error.code(), "validation_error");
        }
    }

    #[tokio::test]
    async fn pull_request_open_returns_pr_number_from_api_response() {
        let gh = FakeGhCommand::default();
        gh.put_api_response(
            "api_post",
            "/repos/owner/repo/pulls",
            json!({
                "number": 42,
                "html_url": "https://github.com/owner/repo/pull/42"
            }),
        );

        let result = invoke_tool_with_gh(
            &gh,
            &write_invoke_params(
                TOOL_PULL_REQUEST_OPEN,
                json!({
                    "repo": "owner/repo",
                    "title": "Open synthetic PR",
                    "head": "feature/synthetic",
                    "base": "main",
                    "body": "Synthetic body"
                }),
            ),
            None,
        )
        .await
        .expect("pull_request_open succeeds");

        assert_eq!(result.output["output"]["number"], 42);
        assert_eq!(
            result.output["output"]["html_url"],
            "https://github.com/owner/repo/pull/42"
        );
        assert!(
            gh.invocations()
                .iter()
                .any(|invocation| invocation.method == "api_post")
        );
    }

    #[tokio::test]
    async fn branch_delete_issues_api_delete_call() {
        let gh = FakeGhCommand::default();

        let result = invoke_tool_with_gh(
            &gh,
            &write_invoke_params(
                TOOL_BRANCH_DELETE,
                json!({
                    "repo": "owner/repo",
                    "branch": "feature/synthetic"
                }),
            ),
            None,
        )
        .await
        .expect("branch_delete succeeds");

        assert_eq!(result.status, "ok");
        assert_eq!(
            gh.invocations()
                .iter()
                .filter(|invocation| invocation.method == "api_delete")
                .count(),
            1
        );
    }

    #[test]
    fn partial_scope_defaults_are_normalized() {
        let mut params = invoke_params(TOOL_LIST_ISSUES, json!({ "repo": "owner/repo" }));
        params.package_scope = json!({
            "mode": "read",
            "auth_mode": "host",
            "allowed_repos": ["owner/repo"],
            "allowed_refs": ["main"],
            "max_results": 12
        });

        let scope = validate_read_scope(&params).expect("partial scope defaults are valid");

        assert_eq!(scope.max_results, 12);
        assert_eq!(scope.max_file_lines, DEFAULT_MAX_FILE_LINES);
        assert_eq!(scope.max_file_bytes, DEFAULT_MAX_FILE_BYTES);
        assert_eq!(scope.max_matches, DEFAULT_MAX_MATCHES);
        assert_eq!(scope.context_lines, DEFAULT_CONTEXT_LINES);
        assert_eq!(scope.max_clone_bytes, DEFAULT_MAX_CLONE_BYTES);
    }

    #[test]
    fn unrestricted_scope_accepts_bridge_metadata_and_allows_any_repo_ref() {
        let mut params = invoke_params(
            TOOL_FETCH_FILE,
            json!({
                "repo": "other-owner/other-repo",
                "path": "README.md",
                "ref": "feature/e2e"
            }),
        );
        params.package_scope = json!({
            "mode": "read",
            "auth_mode": "host",
            "unrestricted": true,
            "connection_id": "019f0000-0000-7000-8000-000000000000",
            "connection_alias_prefix": "github",
            "tool_alias": "github.fetch_file",
            "runtime_commands": ["gh", "git"],
            "connection_scope": { "unrestricted": true },
            "grant_scope": {}
        });

        let scope = validate_read_scope(&params).expect("unrestricted scope is valid");

        validate_repo_allowed("other-owner/other-repo", &scope).expect("any repo is allowed");
        validate_ref_allowed("feature/e2e", &scope).expect("any ref is allowed");
    }

    #[test]
    fn scoped_scope_still_requires_allowlists() {
        let mut params = invoke_params(TOOL_LIST_ISSUES, json!({ "repo": "owner/repo" }));
        params.package_scope = json!({
            "mode": "read",
            "auth_mode": "host"
        });

        let error = validate_read_scope(&params).expect_err("scoped grants require allowlists");

        assert_eq!(error.code(), "validation_error");
    }

    #[test]
    fn unrestricted_write_scope_accepts_send_mode_without_allowlists() {
        let mut params = write_invoke_params(
            TOOL_BRANCH_CREATE,
            json!({
                "repo": "other-owner/other-repo",
                "branch": "phase25-e2e"
            }),
        );
        params.package_scope = json!({
            "mode": "send",
            "auth_mode": "host",
            "unrestricted": true,
            "connection_scope": { "unrestricted": true },
            "grant_scope": {}
        });

        let scope = validate_write_scope(&params).expect("unrestricted send scope is valid");

        validate_repo_allowed("other-owner/other-repo", &scope).expect("any repo is allowed");
    }

    #[tokio::test]
    async fn remote_write_tools_reject_read_scope() {
        let gh = FakeGhCommand::default();
        let cases = [
            (
                TOOL_PULL_REQUEST_OPEN,
                json!({
                    "repo": "owner/repo",
                    "title": "Open synthetic PR",
                    "head": "feature/synthetic",
                    "base": "main"
                }),
            ),
            (
                TOOL_PULL_REQUEST_MERGE,
                json!({
                    "repo": "owner/repo",
                    "number": 42
                }),
            ),
            (
                TOOL_COMMENT,
                json!({
                    "repo": "owner/repo",
                    "issue_or_pr_number": 42,
                    "body": "Synthetic comment"
                }),
            ),
            (
                TOOL_ISSUE_CREATE,
                json!({
                    "repo": "owner/repo",
                    "title": "Synthetic issue"
                }),
            ),
            (
                TOOL_LABEL_EDIT,
                json!({
                    "repo": "owner/repo",
                    "name": "synthetic",
                    "color": "0055aa"
                }),
            ),
            (
                TOOL_BRANCH_DELETE,
                json!({
                    "repo": "owner/repo",
                    "branch": "feature/synthetic"
                }),
            ),
        ];

        for (tool_id, arguments) in cases {
            let error = invoke_tool_with_gh(&gh, &invoke_params(tool_id, arguments), None)
                .await
                .expect_err("read grant rejects remote write tool");
            assert_eq!(error.code(), "validation_error");
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn invoke_params(tool_id: &str, arguments: Value) -> ToolInvokeParams {
        ToolInvokeParams {
            tool_id: ToolId::new(tool_id).expect("valid tool id"),
            run_id: RunId::new(),
            grant_id: GrantId::new(),
            mode: CapabilityMode::Read,
            package_scope: json!({
                "mode": "read",
                "auth_mode": "host",
                "allowed_repos": ["owner/repo"],
                "allowed_refs": ["main"],
                "max_file_lines": 120,
                "max_file_bytes": 32768,
                "max_matches": 20,
                "max_results": 30,
                "context_lines": 3,
                "max_clone_bytes": 524288000
            }),
            arguments,
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn write_invoke_params(tool_id: &str, arguments: Value) -> ToolInvokeParams {
        ToolInvokeParams {
            tool_id: ToolId::new(tool_id).expect("valid tool id"),
            run_id: RunId::new(),
            grant_id: GrantId::new(),
            mode: CapabilityMode::Send,
            package_scope: json!({
                "mode": "write",
                "auth_mode": "host",
                "allowed_repos": ["owner/repo"],
                "allowed_refs": ["main"],
                "max_file_lines": 120,
                "max_file_bytes": 32768,
                "max_matches": 20,
                "max_results": 30,
                "context_lines": 3,
                "max_clone_bytes": 524288000
            }),
            arguments,
        }
    }

    fn test_otto_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "otto-github-{name}-{}-{}",
            std::process::id(),
            GrantId::new()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test OTTO_ROOT");
        path
    }
}
