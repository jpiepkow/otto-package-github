use otto_extension_sdk::grants::CapabilityMode;
use otto_extension_sdk::protocol::{
    METHOD_HANDSHAKE, METHOD_HEALTH, METHOD_REGISTRATIONS_GET, METHOD_SETUP_CHECKS_RUN,
    METHOD_SHUTDOWN, METHOD_TOOLS_INVOKE,
};
use otto_extension_sdk::rpc::framing::{read_rpc_frame, write_rpc_frame};
use otto_tool_github::{
    CAPABILITY_READ, CAPABILITY_WRITE, DISPLAY_NAME, PACKAGE_ID, ROLE_ID, SETUP_READY,
    TOOL_GIT_PUSH, TOOL_LIST_ISSUES, TOOL_PREPARE_CHECKOUT, TOOL_PULL_REQUEST_MERGE,
    command::{GhCommand, TokioGhCommand},
    read_tool_ids, write_tool_ids,
};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const WRITE_TOOL_IDS_UNDER_TEST: [&str; 10] = [
    "tool.default.github.git_stage",
    "tool.default.github.git_commit",
    "tool.default.github.branch_create",
    "tool.default.github.git_push",
    "tool.default.github.pull_request_open",
    "tool.default.github.pull_request_merge",
    "tool.default.github.comment",
    "tool.default.github.issue_create",
    "tool.default.github.label_edit",
    "tool.default.github.branch_delete",
];

#[tokio::test]
async fn github_tool_runtime_fake_mode() -> anyhow::Result<()> {
    assert_eq!(CAPABILITY_READ, "cap.default.github.read");
    assert_eq!(CAPABILITY_WRITE, "cap.default.github.write");

    let mut process = spawn_runtime()?;

    let registrations = assert_extension_protocol_ready(&mut process).await?;
    assert_github_registrations(&registrations);
    assert_read_tools_no_approval(&registrations);
    assert_write_tools_require_approval(&registrations);
    assert_each_write_tool_rejects_read_grant(&mut process).await?;
    assert_prepare_checkout_mount_path(&mut process).await?;

    shutdown(process).await
}

#[tokio::test]
#[ignore = "live: requires authenticated gh"]
async fn live_gh_auth_status() -> anyhow::Result<()> {
    let gh = TokioGhCommand::default();
    gh.version()
        .await
        .map_err(|error| anyhow::anyhow!("live smoke requires gh on PATH: {error}"))?;
    let status = gh
        .auth_status(None)
        .await
        .map_err(|error| anyhow::anyhow!("live smoke failed to run gh auth status: {error}"))?;

    assert!(
        status.authenticated,
        "live smoke requires authenticated gh; run `gh auth login` before `cargo test --manifest-path extensions/com.otto.github/Cargo.toml --test runtime_contract -- --ignored live`"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live: hits public GitHub API"]
async fn live_gh_api_repo() -> anyhow::Result<()> {
    let response = TokioGhCommand::default()
        .api_get("/repos/octocat/Hello-World", None)
        .await
        .map_err(|error| {
            anyhow::anyhow!("live smoke failed to run gh api /repos/octocat/Hello-World: {error}")
        })?;

    assert_eq!(response["full_name"], "octocat/Hello-World");
    Ok(())
}

#[tokio::test]
#[ignore = "live: requires authenticated gh and starts the package without --fake"]
async fn live_runtime_real_mode_setup_check_and_read_tool() -> anyhow::Result<()> {
    let mut process = spawn_real_runtime()?;

    let _registrations = assert_extension_protocol_ready_real_mode(&mut process).await?;
    let response = request(
        &mut process.stdin,
        &mut process.stdout,
        40,
        METHOD_TOOLS_INVOKE,
        Some(invoke_params(
            TOOL_LIST_ISSUES,
            CapabilityMode::Read,
            unrestricted_read_package_scope(),
            json!({
                "repo": "octocat/Hello-World",
                "state": "open",
                "max_results": 1
            }),
        )),
    )
    .await?;

    assert_eq!(response["result"]["is_error"], false, "{response}");
    assert_eq!(
        response["result"]["structured_content"]["status"], "ok",
        "{response}"
    );
    assert_eq!(
        response["result"]["structured_content"]["output"]["repo"],
        "octocat/Hello-World"
    );
    shutdown(process).await
}

async fn assert_extension_protocol_ready(process: &mut RuntimeProcess) -> anyhow::Result<Value> {
    let handshake = request(
        &mut process.stdin,
        &mut process.stdout,
        1,
        METHOD_HANDSHAKE,
        Some(json!({
            "protocol_version": "otto.extension.rpc.v1",
            "package_id": PACKAGE_ID
        })),
    )
    .await?;
    assert_eq!(handshake["result"]["package_id"], PACKAGE_ID);
    assert_eq!(handshake["result"]["display_name"], DISPLAY_NAME);

    let health = request(
        &mut process.stdin,
        &mut process.stdout,
        2,
        METHOD_HEALTH,
        None,
    )
    .await?;
    assert_eq!(health["result"]["healthy"], true);

    let setup = request(
        &mut process.stdin,
        &mut process.stdout,
        3,
        METHOD_SETUP_CHECKS_RUN,
        Some(json!({ "setup_check_id": SETUP_READY })),
    )
    .await?;
    assert_eq!(setup["result"]["setup_check_id"], SETUP_READY);
    assert_eq!(setup["result"]["ok"], true);
    assert_eq!(setup["result"]["details"]["mode"], "read");
    assert_eq!(setup["result"]["details"]["fake_mode"], true);
    assert_eq!(setup["result"]["details"]["status"], "ok");

    let registrations = request(
        &mut process.stdin,
        &mut process.stdout,
        4,
        METHOD_REGISTRATIONS_GET,
        None,
    )
    .await?;
    Ok(registrations["result"]["registrations"].clone())
}

async fn assert_extension_protocol_ready_real_mode(
    process: &mut RuntimeProcess,
) -> anyhow::Result<Value> {
    let handshake = request(
        &mut process.stdin,
        &mut process.stdout,
        1,
        METHOD_HANDSHAKE,
        Some(json!({
            "protocol_version": "otto.extension.rpc.v1",
            "package_id": PACKAGE_ID
        })),
    )
    .await?;
    assert_eq!(handshake["result"]["package_id"], PACKAGE_ID);
    assert_eq!(handshake["result"]["display_name"], DISPLAY_NAME);

    let health = request(
        &mut process.stdin,
        &mut process.stdout,
        2,
        METHOD_HEALTH,
        None,
    )
    .await?;
    assert_eq!(health["result"]["healthy"], true);

    let setup = request(
        &mut process.stdin,
        &mut process.stdout,
        3,
        METHOD_SETUP_CHECKS_RUN,
        Some(json!({ "setup_check_id": SETUP_READY })),
    )
    .await?;
    assert_eq!(setup["result"]["setup_check_id"], SETUP_READY);
    assert_eq!(setup["result"]["ok"], true);
    assert_eq!(setup["result"]["details"]["fake_mode"], false);
    assert_eq!(setup["result"]["details"]["status"], "ok");

    let registrations = request(
        &mut process.stdin,
        &mut process.stdout,
        4,
        METHOD_REGISTRATIONS_GET,
        None,
    )
    .await?;
    Ok(registrations["result"]["registrations"].clone())
}

fn assert_github_registrations(registrations: &Value) {
    assert_eq!(registrations["roles"][0]["id"], ROLE_ID);
    assert_eq!(registrations["roles"][0]["kind"], "tool_package");
    assert_eq!(
        registrations["roles"][0]["capabilities"],
        json!([CAPABILITY_READ, CAPABILITY_WRITE])
    );

    let capabilities = registrations["capabilities"]
        .as_array()
        .expect("capabilities array");
    let read_capability = capabilities
        .iter()
        .find(|capability| capability["id"] == CAPABILITY_READ)
        .expect("read capability");
    assert_eq!(read_capability["mode"], "read");
    let write_capability = capabilities
        .iter()
        .find(|capability| capability["id"] == CAPABILITY_WRITE)
        .expect("write capability");
    assert_eq!(write_capability["mode"], "send");

    let tools = registrations["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 16);
    for tool_id in read_tool_ids().into_iter().chain(write_tool_ids()) {
        assert!(
            tools.iter().any(|tool| tool["id"] == tool_id),
            "missing tool registration {tool_id}"
        );
    }

    assert_eq!(registrations["setup_checks"][0]["id"], SETUP_READY);
    assert_eq!(
        registrations["setup_checks"][0]["output_schema"],
        "schema.default.github.setup_details"
    );
    assert_eq!(
        registrations["schemas"]
            .as_array()
            .expect("schemas array")
            .len(),
        23
    );
    assert_eq!(
        registrations["ui_forms"]
            .as_array()
            .expect("ui forms array")
            .iter()
            .map(|form| form["id"].as_str().expect("ui form id"))
            .collect::<Vec<_>>(),
        vec!["github_setup", "github_grant"]
    );
    assert_eq!(registrations["triggers"], json!([]));
    assert_eq!(registrations["redaction"], json!([]));
    assert_eq!(registrations["migrations"], json!([]));
}

fn assert_read_tools_no_approval(registrations: &Value) {
    let tools = registrations["tools"].as_array().expect("tools array");
    for tool_id in read_tool_ids() {
        let tool = tools
            .iter()
            .find(|tool| tool["id"] == tool_id)
            .expect("read tool registration");
        assert_eq!(tool["requires_approval"], false, "{tool_id}");
        assert_eq!(tool["read_only"], true, "{tool_id}");
        assert_eq!(tool["destructive"], false, "{tool_id}");
        assert_eq!(tool["open_world"], true, "{tool_id}");
        assert_eq!(tool["required_capabilities"], json!([CAPABILITY_READ]));
    }
}

fn assert_write_tools_require_approval(registrations: &Value) {
    assert_eq!(WRITE_TOOL_IDS_UNDER_TEST, write_tool_ids());
    let tools = registrations["tools"].as_array().expect("tools array");
    for tool_id in WRITE_TOOL_IDS_UNDER_TEST {
        let tool = tools
            .iter()
            .find(|tool| tool["id"] == tool_id)
            .expect("write tool registration");
        assert_eq!(tool["requires_approval"], true, "{tool_id}");
        assert_eq!(tool["read_only"], false, "{tool_id}");
        assert_eq!(tool["destructive"], true, "{tool_id}");
        assert_eq!(
            tool["open_world"],
            !matches!(
                tool_id,
                "tool.default.github.git_stage"
                    | "tool.default.github.git_commit"
                    | "tool.default.github.branch_create"
            ),
            "{tool_id}"
        );
        assert_eq!(tool["required_capabilities"], json!([CAPABILITY_WRITE]));
    }
}

async fn assert_each_write_tool_rejects_read_grant(
    process: &mut RuntimeProcess,
) -> anyhow::Result<()> {
    let cases = [
        (
            TOOL_GIT_PUSH,
            json!({
                "repo": "owner/repo",
                "refspec": "HEAD:refs/heads/feature",
                "force": true
            }),
        ),
        (
            TOOL_PULL_REQUEST_MERGE,
            json!({
                "repo": "owner/repo",
                "number": 42
            }),
        ),
    ];

    for (index, (tool_id, arguments)) in cases.into_iter().enumerate() {
        let response = request(
            &mut process.stdin,
            &mut process.stdout,
            30 + index as u64,
            METHOD_TOOLS_INVOKE,
            Some(invoke_params(
                tool_id,
                CapabilityMode::Read,
                read_package_scope(),
                arguments,
            )),
        )
        .await?;
        assert_eq!(response["error"], Value::Null, "{response}");
        assert_eq!(response["result"]["is_error"], true, "{response}");
        assert_eq!(
            response["result"]["structured_content"],
            Value::Null,
            "{response}"
        );
        assert!(
            response["result"]["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("GitHub write tools require send mode")),
            "{response}"
        );
    }
    Ok(())
}

async fn assert_prepare_checkout_mount_path(process: &mut RuntimeProcess) -> anyhow::Result<()> {
    let response = request(
        &mut process.stdin,
        &mut process.stdout,
        31,
        METHOD_TOOLS_INVOKE,
        Some(invoke_params(
            TOOL_PREPARE_CHECKOUT,
            CapabilityMode::Read,
            read_package_scope(),
            json!({
                "repo": "owner/repo",
                "ref": "main"
            }),
        )),
    )
    .await?;
    assert_eq!(response["result"]["is_error"], false);
    assert_eq!(
        response["result"]["structured_content"]["output"]["mount_path"],
        "/otto/checkout/owner/repo"
    );
    assert!(
        response["result"]["structured_content"]["output"]["commit_sha"]
            .as_str()
            .is_some_and(|sha| !sha.is_empty())
    );
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn invoke_params(
    tool_id: &str,
    mode: CapabilityMode,
    package_scope: Value,
    arguments: Value,
) -> Value {
    json!({
        "tool_id": tool_id,
        "run_id": "00000000-0000-0000-0000-000000000087",
        "grant_id": "00000000-0000-0000-0000-000000000086",
        "mode": mode,
        "package_scope": package_scope,
        "arguments": arguments
    })
}

fn read_package_scope() -> Value {
    json!({
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
    })
}

fn unrestricted_read_package_scope() -> Value {
    json!({
        "mode": "read",
        "auth_mode": "host",
        "max_results": 5,
        "runtime_commands": ["gh", "git"],
        "connection_scope": { "unrestricted": true },
        "grant_scope": {}
    })
}

#[allow(dead_code)]
fn write_package_scope() -> Value {
    json!({
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
    })
}

struct RuntimeProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

fn spawn_runtime() -> anyhow::Result<RuntimeProcess> {
    spawn_runtime_with_args(["--fake"])
}

fn spawn_real_runtime() -> anyhow::Result<RuntimeProcess> {
    spawn_runtime_with_args(std::iter::empty::<&str>())
}

fn spawn_runtime_with_args(
    args: impl IntoIterator<Item = &'static str>,
) -> anyhow::Result<RuntimeProcess> {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin("otto-tool-github"));
    command
        .args(args)
        .env("OTTO_ROOT", runtime_otto_root()?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let stdin = child.stdin.take().expect("runtime stdin");
    let stdout = child.stdout.take().expect("runtime stdout");
    Ok(RuntimeProcess {
        child,
        stdin,
        stdout,
    })
}

fn runtime_otto_root() -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "otto-github-runtime-contract-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path)?;
    Ok(path)
}

async fn request(
    stdin: &mut ChildStdin,
    stdout: &mut ChildStdout,
    id: u64,
    method: &str,
    params: Option<Value>,
) -> anyhow::Result<Value> {
    let mut message = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method
    });
    if let Some(params) = params {
        message["params"] = params;
    }
    write_rpc_frame(stdin, &message).await?;
    Ok(read_rpc_frame(stdout, 64 * 1024).await?)
}

async fn shutdown(mut process: RuntimeProcess) -> anyhow::Result<()> {
    let shutdown = request(
        &mut process.stdin,
        &mut process.stdout,
        999,
        METHOD_SHUTDOWN,
        Some(json!({ "reason": "test complete" })),
    )
    .await?;
    assert_eq!(shutdown["result"]["accepted"], true);
    process.stdin.shutdown().await?;
    let status = process.child.wait().await?;
    assert!(status.success());
    Ok(())
}
