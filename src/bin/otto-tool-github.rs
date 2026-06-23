use otto_extension_sdk::extension_ids::SetupCheckId;
use otto_extension_sdk::protocol::{
    HandshakeParams, HandshakeResult, HealthResult, METHOD_HANDSHAKE, METHOD_HEALTH,
    METHOD_REGISTRATIONS_GET, METHOD_SETUP_CALL, METHOD_SETUP_CHECKS_RUN,
    METHOD_SETUP_FORM_CONFIGURATION, METHOD_SHUTDOWN, METHOD_TOOLS_INVOKE, RegistrationsResult,
    SetupCallParams, SetupCallResult, SetupCallSpec, SetupCheckRunParams, SetupCheckRunResult,
    SetupFormConfigurationParams, SetupFormConfigurationResult, ShutdownResult, ToolInvokeParams,
};
use otto_extension_sdk::rpc::framing::{read_rpc_frame, write_rpc_frame};
use otto_tool_github::{
    DISPLAY_NAME, FAKE_MODE_ARG, PACKAGE_ID, SETUP_READY,
    command::{FakeGhCommand, TokioGhCommand},
    invoke_tool_with_gh, registrations,
    setup_check::GithubSetupCheck,
};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;
use tokio::io::{AsyncWrite, AsyncWriteExt};

const MAX_FRAME_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("otto-tool-github error: {error}");
            std::process::ExitCode::from(1)
        }
    }
}

async fn run() -> RuntimeResult<()> {
    let fake_mode = fake_mode_enabled();
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    loop {
        let frame = read_rpc_frame(&mut stdin, MAX_FRAME_BYTES)
            .await
            .map_err(|_| RuntimeError::ExternalIo {
                summary: "tool package JSON-RPC frame read failed".to_owned(),
            })?;
        let request =
            serde_json::from_value::<JsonRpcRequest>(frame).map_err(|_| RuntimeError::Invalid {
                summary: "tool package JSON-RPC request was malformed".to_owned(),
            })?;
        if request.jsonrpc != "2.0" {
            let response = response(
                request.id,
                Err(RuntimeError::Denied {
                    reason: "unsupported JSON-RPC version".to_owned(),
                }),
            );
            write_response(&mut stdout, &response).await?;
            continue;
        }

        let should_shutdown = request.method == METHOD_SHUTDOWN;
        let response = handle_request(request, fake_mode).await;
        write_response(&mut stdout, &response).await?;
        if should_shutdown {
            break;
        }
    }

    Ok(())
}

async fn handle_request(request: JsonRpcRequest, fake_mode: bool) -> Value {
    let result = match request.method.as_str() {
        METHOD_HANDSHAKE => handshake(request.params),
        METHOD_HEALTH => Ok(json!(HealthResult {
            healthy: true,
            status: "ok".to_owned(),
            reason: None,
        })),
        METHOD_SETUP_CHECKS_RUN => run_setup(request.params, fake_mode).await,
        METHOD_SETUP_FORM_CONFIGURATION => setup_form_configuration(request.params),
        METHOD_SETUP_CALL => setup_call(request.params, fake_mode),
        METHOD_REGISTRATIONS_GET => Ok(json!(RegistrationsResult {
            registrations: registrations(),
        })),
        METHOD_TOOLS_INVOKE => invoke_tool(request.params, fake_mode).await,
        METHOD_SHUTDOWN => Ok(json!(ShutdownResult { accepted: true })),
        _ => Err(RuntimeError::Denied {
            reason: "unknown tool package method".to_owned(),
        }),
    };
    response(request.id, result)
}

fn handshake(params: Option<Value>) -> RuntimeResult<Value> {
    let params = decode_params::<HandshakeParams>(params)?;
    if params.package_id.as_str() != PACKAGE_ID {
        return Err(RuntimeError::Denied {
            reason: "handshake package id mismatch".to_owned(),
        });
    }
    Ok(json!(HandshakeResult {
        protocol_version: params.protocol_version,
        package_id: params.package_id,
        display_name: DISPLAY_NAME.to_owned(),
    }))
}

async fn run_setup(params: Option<Value>, fake_mode: bool) -> RuntimeResult<Value> {
    let params = decode_params::<SetupCheckRunParams>(params)?;
    if params.setup_check_id.as_str() != SETUP_READY {
        return Err(RuntimeError::Denied {
            reason: "unknown GitHub setup check".to_owned(),
        });
    }

    if !fake_mode {
        let result = GithubSetupCheck::new(TokioGhCommand::default())
            .run(None)
            .await
            .map_err(|error| RuntimeError::ExternalIo {
                summary: error.to_string(),
            })?;
        return Ok(json!(SetupCheckRunResult {
            setup_check_id: setup_check_id(SETUP_READY),
            ok: result.ok,
            message: result.message,
            details: result.details,
        }));
    }

    Ok(json!(SetupCheckRunResult {
        setup_check_id: setup_check_id(SETUP_READY),
        ok: true,
        message: Some("GitHub fake tools are ready".to_owned()),
        details: json!({
            "mode": "read",
            "fake_mode": true,
            "gh_version": "gh version 2.72.0 (fake)",
            "gh_authenticated": true,
            "auth_mode": "host",
            "status": "ok",
        }),
    }))
}

fn setup_form_configuration(params: Option<Value>) -> RuntimeResult<Value> {
    let _params = decode_params::<SetupFormConfigurationParams>(params)?;
    let form = json!({
        "form_id": "github_setup",
        "title": "GitHub tools",
        "description": "Configure host gh or token-backed GitHub access.",
        "fields": [
            {
                "name": "auth_mode",
                "label": "Authentication mode",
                "kind": "select",
                "required": true,
                "default": "host",
                "options": ["host", "token"]
            },
            {
                "name": "credential_ref",
                "label": "Credential reference",
                "kind": "text",
                "required": false,
                "description": "Required only for token mode."
            }
        ],
        "fixture": {
            "auth_mode": "host",
            "fake_mode": true
        }
    });
    Ok(json!(SetupFormConfigurationResult {
        form,
        calls: vec![SetupCallSpec {
            id: "check_ready".to_owned(),
            kind: "check".to_owned(),
            display_name: "Check GitHub tools".to_owned(),
            description: Some("Runs package setup checks for the GitHub package.".to_owned()),
            input_schema: None,
            output_schema: None,
            blocks_continue: false,
        }],
    }))
}

fn setup_call(params: Option<Value>, fake_mode: bool) -> RuntimeResult<Value> {
    let params = decode_params::<SetupCallParams>(params)?;
    match params.call_id.as_str() {
        "check_ready" => Ok(json!(SetupCallResult {
            status: "ok".to_owned(),
            message: Some(if fake_mode {
                "GitHub fake tools are ready.".to_owned()
            } else {
                "GitHub tools are ready.".to_owned()
            }),
            output: json!({
                "fake_mode": fake_mode,
                "supported_tools": otto_tool_github::tool_ids(),
            }),
        })),
        _ => Err(RuntimeError::Denied {
            reason: "unknown GitHub setup call".to_owned(),
        }),
    }
}

async fn invoke_tool(params: Option<Value>, fake_mode: bool) -> RuntimeResult<Value> {
    let params = decode_params::<ToolInvokeParams>(params)?;
    if fake_mode {
        let gh = FakeGhCommand::default();
        return Ok(json!(invoke_tool_with_gh(&gh, &params, None).await?));
    }

    let token = github_token(&params)?;
    let gh = TokioGhCommand::default();
    Ok(json!(
        invoke_tool_with_gh(&gh, &params, token.as_ref()).await?
    ))
}

fn github_token(params: &ToolInvokeParams) -> RuntimeResult<Option<SecretString>> {
    if params
        .package_scope
        .get("auth_mode")
        .and_then(Value::as_str)
        != Some("token")
    {
        return Ok(None);
    }
    let Some(token) = secret(&params.package_scope, "credential") else {
        return Err(RuntimeError::Validation {
            reason: "GitHub token auth requires materialized credential secret".to_owned(),
        });
    };
    Ok(Some(SecretString::from(token)))
}

fn secret(scope: &Value, name: &str) -> Option<String> {
    scope
        .get("_otto_secrets")
        .and_then(Value::as_object)
        .and_then(|secrets| {
            secrets
                .get(name)
                .or_else(|| secrets.get(&format!("{name}_ref")))
                .or_else(|| secrets.get(&format!("{name}_credential_ref")))
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn decode_params<T>(params: Option<Value>) -> RuntimeResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(params.unwrap_or(Value::Null)).map_err(|_| RuntimeError::Invalid {
        summary: "tool package request params did not match method".to_owned(),
    })
}

#[allow(clippy::needless_pass_by_value)]
fn response(id: Value, result: RuntimeResult<Value>) -> Value {
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": error.code()
            }
        }),
    }
}

async fn write_response<W>(writer: &mut W, response: &Value) -> RuntimeResult<()>
where
    W: AsyncWrite + Unpin,
{
    write_rpc_frame(writer, response)
        .await
        .map_err(|_| RuntimeError::ExternalIo {
            summary: "tool package JSON-RPC frame write failed".to_owned(),
        })?;
    writer.flush().await.map_err(|_| RuntimeError::ExternalIo {
        summary: "tool package JSON-RPC flush failed".to_owned(),
    })
}

fn fake_mode_enabled() -> bool {
    std::env::args().any(|arg| arg == FAKE_MODE_ARG)
        || std::env::var("OTTO_TOOL_GITHUB_FAKE").is_ok_and(|value| value == "1")
}

fn setup_check_id(value: &str) -> SetupCheckId {
    SetupCheckId::new(value).expect("valid setup check id")
}

type RuntimeResult<T> = Result<T, RuntimeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeError {
    Denied { reason: String },
    ExternalIo { summary: String },
    Invalid { summary: String },
    Validation { reason: String },
}

impl RuntimeError {
    const fn code(&self) -> &'static str {
        match self {
            Self::Denied { .. } => "denied",
            Self::ExternalIo { .. } => "external_io",
            Self::Invalid { .. } => "invalid_request",
            Self::Validation { .. } => "validation_error",
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied { reason } => write!(formatter, "operation denied: {reason}"),
            Self::ExternalIo { summary } => write!(formatter, "external I/O failed: {summary}"),
            Self::Invalid { summary } => write!(formatter, "invalid request: {summary}"),
            Self::Validation { reason } => write!(formatter, "validation error: {reason}"),
        }
    }
}

impl Error for RuntimeError {}

impl From<otto_tool_github::ToolRuntimeError> for RuntimeError {
    fn from(error: otto_tool_github::ToolRuntimeError) -> Self {
        match error {
            otto_tool_github::ToolRuntimeError::Validation { reason } => {
                Self::Validation { reason }
            }
            otto_tool_github::ToolRuntimeError::External { summary } => {
                Self::ExternalIo { summary }
            }
        }
    }
}
