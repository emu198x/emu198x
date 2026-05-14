//! Cross-system Model Context Protocol server framework.
//!
//! Hand-rolled JSON-RPC 2.0 over stdio with the MCP envelope. The shell
//! crate owns the protocol-level pieces (wire types, the [`Tool`] trait,
//! a [`ToolRegistry`]); per-system binaries register concrete tools that
//! delegate into their existing script-step interceptors and the shell's
//! script executor.
//!
//! This module covers the foundational building blocks. Server-side
//! method dispatch (`initialize`, `tools/list`, `tools/call`) and the
//! stdio loop land in a sibling commit alongside the Spectrum binary's
//! tool registrations; see
//! `docs/brainstorms/2026-05-08-mcp-server-brainstorm.md` for the full
//! phase shape.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

/// JSON-RPC 2.0 protocol version literal.
pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC standard error codes.
pub mod error_code {
    /// Invalid JSON received by the server.
    pub const PARSE_ERROR: i32 = -32700;
    /// JSON sent is not a valid request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// JSON-RPC request id. Number or string per the spec; absent for
/// notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    /// Numeric id.
    Number(i64),
    /// String id.
    String(String),
}

/// JSON-RPC 2.0 request frame.
///
/// Notifications use the same shape with `id` omitted. Servers must not
/// reply to notifications.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    /// Protocol version literal — always `"2.0"`.
    pub jsonrpc: String,
    /// Request id. `None` indicates a notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<JsonRpcId>,
    /// Method name (e.g. `"initialize"`, `"tools/call"`).
    pub method: String,
    /// Method parameters. Method-specific shape; `None` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Returns whether this frame is a notification (no `id`).
    #[must_use]
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// JSON-RPC 2.0 response frame.
///
/// Exactly one of `result` or `error` must be set.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcResponse {
    /// Protocol version literal — always `"2.0"`.
    pub jsonrpc: String,
    /// The request id this response corresponds to.
    pub id: JsonRpcId,
    /// Successful method result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error envelope when the call failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Builds a success response carrying the supplied result payload.
    #[must_use]
    pub fn success(id: JsonRpcId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Builds an error response from a [`JsonRpcError`].
    #[must_use]
    pub fn error(id: JsonRpcId, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// JSON-RPC 2.0 error envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcError {
    /// Numeric error code per the JSON-RPC spec.
    pub code: i32,
    /// Short human-readable description.
    pub message: String,
    /// Optional structured error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Builds a parse-error envelope.
    #[must_use]
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: error_code::PARSE_ERROR,
            message: message.into(),
            data: None,
        }
    }

    /// Builds an invalid-request envelope.
    #[must_use]
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: error_code::INVALID_REQUEST,
            message: message.into(),
            data: None,
        }
    }

    /// Builds a method-not-found envelope.
    #[must_use]
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Self {
            code: error_code::METHOD_NOT_FOUND,
            message: format!("method not found: {}", method.into()),
            data: None,
        }
    }

    /// Builds an invalid-params envelope.
    #[must_use]
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: error_code::INVALID_PARAMS,
            message: message.into(),
            data: None,
        }
    }

    /// Builds an internal-error envelope.
    #[must_use]
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: error_code::INTERNAL_ERROR,
            message: message.into(),
            data: None,
        }
    }
}

/// One content block returned by a tool. MCP `tools/call` responses
/// carry an array of these in the `content` field.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    /// UTF-8 text payload.
    Text {
        /// The text body.
        text: String,
    },
}

impl ToolContent {
    /// Builds a text content block.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
}

/// MCP `tools/call` result envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolResponse {
    /// Content blocks the client should render.
    pub content: Vec<ToolContent>,
    /// Whether the tool execution itself failed (as opposed to a
    /// protocol-level error). Defaults to `false` when omitted.
    #[serde(default, rename = "isError", skip_serializing_if = "is_false")]
    pub is_error: bool,
}

impl ToolResponse {
    /// Builds a success response from one text block.
    #[must_use]
    pub fn success_text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::text(text)],
            is_error: false,
        }
    }

    /// Builds an error response from one text block. The JSON-RPC envelope
    /// stays a successful response per the MCP spec — only the
    /// `isError` flag flips. Use [`JsonRpcError`] for protocol-level
    /// failures.
    #[must_use]
    pub fn error_text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::text(text)],
            is_error: true,
        }
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

/// Failure produced by a tool's `call`. The MCP dispatcher converts
/// this into a `tools/call` response with `isError: true`.
#[derive(Debug, Error)]
pub enum ToolError {
    /// The tool's input arguments were malformed.
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    /// The tool's underlying operation failed.
    #[error("{0}")]
    Execution(String),
}

/// One MCP tool. Tools are registered with a [`ToolRegistry`] and
/// invoked through the server's `tools/call` dispatcher. The `C`
/// type parameter is the binary's chosen execution context (typically
/// the live session or a wrapper around it) — the shell stays
/// agnostic to what binaries put in there.
pub trait Tool<C>: Send + Sync {
    /// Stable tool name, used as the MCP `name` field.
    fn name(&self) -> &str;

    /// Human-readable description shown by MCP clients.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's input arguments.
    fn input_schema(&self) -> serde_json::Value;

    /// Executes the tool against the supplied context and returns
    /// either a successful [`ToolResponse`] or a [`ToolError`].
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::InvalidArguments`] when `arguments` cannot
    /// be parsed into the tool's expected shape, or
    /// [`ToolError::Execution`] when the underlying operation fails.
    fn call(
        &self,
        arguments: serde_json::Value,
        context: &mut C,
    ) -> Result<ToolResponse, ToolError>;
}

/// Tool descriptor returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Stable tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool's input arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Insertion-ordered registry of tools. The order is the order of
/// `tools/list` output; lookup by name is `O(1)`.
pub struct ToolRegistry<C> {
    tools: Vec<Box<dyn Tool<C>>>,
    index: HashMap<String, usize>,
}

impl<C> Default for ToolRegistry<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> ToolRegistry<C> {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Registers one tool. Replaces any tool already registered under
    /// the same name; the most recent registration wins.
    pub fn register(&mut self, tool: Box<dyn Tool<C>>) {
        let name = tool.name().to_owned();
        if let Some(&existing) = self.index.get(&name) {
            self.tools[existing] = tool;
        } else {
            let idx = self.tools.len();
            self.tools.push(tool);
            self.index.insert(name, idx);
        }
    }

    /// Returns the registered tool with the given name, or `None`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Tool<C>> {
        self.index
            .get(name)
            .and_then(|&idx| self.tools.get(idx).map(|t| t.as_ref()))
    }

    /// Iterates registered tools in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Tool<C>> {
        self.tools.iter().map(|t| t.as_ref())
    }

    /// Returns one [`ToolDescriptor`] per registered tool, in
    /// insertion order, suitable for the MCP `tools/list` response.
    #[must_use]
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.iter()
            .map(|tool| ToolDescriptor {
                name: tool.name().to_owned(),
                description: tool.description().to_owned(),
                input_schema: tool.input_schema(),
            })
            .collect()
    }

    /// Returns the number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// MCP protocol version implemented by this server.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Server identity reported in the `initialize` response.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    /// Stable server name (e.g. `"emu198x-spectrum"`).
    pub name: String,
    /// Server version string (typically the cargo `pkg_version!`).
    pub version: String,
}

impl ServerInfo {
    /// Builds a [`ServerInfo`] from string-like parts.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// MCP server: holds the tool registry, handles JSON-RPC method
/// dispatch, and tracks the initialize-handshake state.
pub struct Server<C> {
    info: ServerInfo,
    registry: ToolRegistry<C>,
    initialized: bool,
}

impl<C> Server<C> {
    /// Builds a new server with an empty registry.
    #[must_use]
    pub fn new(info: ServerInfo) -> Self {
        Self {
            info,
            registry: ToolRegistry::new(),
            initialized: false,
        }
    }

    /// Returns a mutable handle on the registry so the binary can
    /// register its tools after construction.
    pub fn registry_mut(&mut self) -> &mut ToolRegistry<C> {
        &mut self.registry
    }

    /// Returns a read-only handle on the registry.
    #[must_use]
    pub fn registry(&self) -> &ToolRegistry<C> {
        &self.registry
    }

    /// Returns whether the client has completed the initialize handshake.
    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Dispatches one JSON-RPC frame.
    ///
    /// Returns `Some(response)` for requests and `None` for notifications
    /// (per JSON-RPC 2.0; servers must not respond to notifications).
    pub fn handle(&mut self, request: JsonRpcRequest, context: &mut C) -> Option<JsonRpcResponse> {
        if request.is_notification() {
            self.handle_notification(&request);
            return None;
        }

        let id = request.id.clone()?;
        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(request.params, context),
            other => Err(JsonRpcError::method_not_found(other)),
        };

        Some(match response {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(error) => JsonRpcResponse::error(id, error),
        })
    }

    fn handle_notification(&mut self, request: &JsonRpcRequest) {
        if request.method == "notifications/initialized" {
            self.initialized = true;
        }
    }

    fn handle_initialize(
        &mut self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
            },
            "serverInfo": {
                "name": self.info.name,
                "version": self.info.version,
            },
        }))
    }

    fn handle_tools_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        Ok(json!({ "tools": self.registry.descriptors() }))
    }

    fn handle_tools_call(
        &self,
        params: Option<serde_json::Value>,
        context: &mut C,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params = params
            .ok_or_else(|| JsonRpcError::invalid_params("tools/call requires a params object"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("tools/call params.name is required"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| JsonRpcError::invalid_params(format!("unknown tool: {name}")))?;

        let response = match tool.call(arguments, context) {
            Ok(response) => response,
            Err(err) => ToolResponse::error_text(err.to_string()),
        };
        serde_json::to_value(&response).map_err(|err| {
            JsonRpcError::internal_error(format!("failed to serialize tool response: {err}"))
        })
    }
}

/// Reads newline-delimited JSON-RPC frames from `reader`, dispatches
/// each one through `server`, and writes responses to `writer`.
///
/// Notifications produce no output; parse errors and dispatch errors
/// produce JSON-RPC error responses (with `id: null` for parse errors,
/// per the spec). The loop ends when `reader` reaches EOF.
///
/// # Errors
///
/// Returns the underlying `io::Error` when reading or writing fails.
/// Tool execution failures are surfaced via `tools/call` responses
/// with `isError: true` rather than as I/O errors.
pub fn serve<R, W, C>(
    server: &mut Server<C>,
    context: &mut C,
    reader: R,
    mut writer: W,
) -> io::Result<()>
where
    R: BufRead,
    W: Write,
{
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = process_line(server, context, &line) {
            writeln!(writer, "{response}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Convenience wrapper around [`serve`] that uses process stdio.
///
/// # Errors
///
/// Propagates any I/O error from stdin or stdout.
pub fn serve_stdio<C>(server: &mut Server<C>, context: &mut C) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(server, context, stdin.lock(), stdout.lock())
}

/// Parses one JSON-RPC line and returns the serialized response (or
/// `None` for notifications).
fn process_line<C>(server: &mut Server<C>, context: &mut C, line: &str) -> Option<String> {
    match serde_json::from_str::<JsonRpcRequest>(line) {
        Ok(request) => {
            let response = server.handle(request, context)?;
            Some(serde_json::to_string(&response).unwrap_or_else(|_| {
                String::from(r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"failed to serialize response"}}"#)
            }))
        }
        Err(err) => Some(parse_error_response(&err.to_string())),
    }
}

fn parse_error_response(message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": null,
        "error": {
            "code": error_code::PARSE_ERROR,
            "message": message,
        }
    }))
    .unwrap_or_else(|_| {
        String::from(
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}}"#,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jsonrpc_request_with_id_round_trips() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let parsed: JsonRpcRequest = serde_json::from_str(raw).expect("parse");
        assert_eq!(parsed.jsonrpc, "2.0");
        assert_eq!(parsed.id, Some(JsonRpcId::Number(1)));
        assert_eq!(parsed.method, "tools/list");
        assert!(!parsed.is_notification());
        let serialized = serde_json::to_string(&parsed).expect("serialize");
        assert!(serialized.contains(r#""id":1"#));
    }

    #[test]
    fn jsonrpc_request_without_id_is_a_notification() {
        let raw = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let parsed: JsonRpcRequest = serde_json::from_str(raw).expect("parse");
        assert!(parsed.is_notification());
        assert_eq!(parsed.method, "notifications/initialized");
    }

    #[test]
    fn jsonrpc_request_accepts_string_id() {
        let raw = r#"{"jsonrpc":"2.0","id":"alpha","method":"ping"}"#;
        let parsed: JsonRpcRequest = serde_json::from_str(raw).expect("parse");
        assert_eq!(parsed.id, Some(JsonRpcId::String("alpha".to_owned())));
    }

    #[test]
    fn jsonrpc_response_success_omits_error_field() {
        let response = JsonRpcResponse::success(JsonRpcId::Number(7), json!({"ok": true}));
        let serialized = serde_json::to_string(&response).expect("serialize");
        assert!(serialized.contains(r#""result":{"ok":true}"#));
        assert!(!serialized.contains("\"error\""));
    }

    #[test]
    fn jsonrpc_response_error_omits_result_field() {
        let response =
            JsonRpcResponse::error(JsonRpcId::Number(7), JsonRpcError::method_not_found("foo"));
        let serialized = serde_json::to_string(&response).expect("serialize");
        assert!(serialized.contains(r#""code":-32601"#));
        assert!(serialized.contains("foo"));
        assert!(!serialized.contains("\"result\""));
    }

    #[test]
    fn tool_response_success_omits_is_error_when_false() {
        let response = ToolResponse::success_text("hello");
        let serialized = serde_json::to_string(&response).expect("serialize");
        assert!(serialized.contains("hello"));
        // is_error: false is omitted via skip_serializing_if so the wire
        // matches the spec's "isError defaults to false" expectation.
        assert!(!serialized.contains("isError"));
    }

    #[test]
    fn tool_response_error_serializes_with_is_error_true() {
        let response = ToolResponse::error_text("bang");
        let serialized = serde_json::to_string(&response).expect("serialize");
        assert!(serialized.contains(r#""isError":true"#));
        assert!(serialized.contains("bang"));
    }

    struct EchoTool {
        name: &'static str,
    }

    impl<C> Tool<C> for EchoTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "echoes the supplied text back as a content block"
        }

        fn input_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
            })
        }

        fn call(
            &self,
            arguments: serde_json::Value,
            _context: &mut C,
        ) -> Result<ToolResponse, ToolError> {
            let text = arguments
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("text must be a string".to_owned()))?;
            Ok(ToolResponse::success_text(text.to_owned()))
        }
    }

    #[test]
    fn tool_registry_preserves_insertion_order() {
        let mut registry: ToolRegistry<()> = ToolRegistry::new();
        registry.register(Box::new(EchoTool { name: "alpha" }));
        registry.register(Box::new(EchoTool { name: "beta" }));
        registry.register(Box::new(EchoTool { name: "gamma" }));

        let names: Vec<_> = registry.iter().map(|t| t.name().to_owned()).collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn tool_registry_get_finds_registered_tools() {
        let mut registry: ToolRegistry<()> = ToolRegistry::new();
        registry.register(Box::new(EchoTool { name: "echo" }));

        let tool = registry.get("echo").expect("tool present");
        let mut ctx = ();
        let response = tool
            .call(json!({"text": "hi"}), &mut ctx)
            .expect("echo should succeed");
        assert!(!response.is_error);
        match &response.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "hi"),
        }
    }

    #[test]
    fn tool_registry_re_register_replaces_tool_in_place() {
        struct StaticTool {
            name: &'static str,
            label: &'static str,
        }

        impl<C> Tool<C> for StaticTool {
            fn name(&self) -> &str {
                self.name
            }
            fn description(&self) -> &str {
                self.label
            }
            fn input_schema(&self) -> serde_json::Value {
                json!({})
            }
            fn call(
                &self,
                _arguments: serde_json::Value,
                _context: &mut C,
            ) -> Result<ToolResponse, ToolError> {
                Ok(ToolResponse::success_text(self.label))
            }
        }

        let mut registry: ToolRegistry<()> = ToolRegistry::new();
        registry.register(Box::new(StaticTool {
            name: "thing",
            label: "first",
        }));
        registry.register(Box::new(StaticTool {
            name: "thing",
            label: "second",
        }));

        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.get("thing").expect("present").description(),
            "second"
        );
    }

    #[test]
    fn tool_registry_descriptors_match_registered_metadata() {
        let mut registry: ToolRegistry<()> = ToolRegistry::new();
        registry.register(Box::new(EchoTool { name: "echo" }));
        registry.register(Box::new(EchoTool { name: "ping" }));

        let descriptors = registry.descriptors();
        assert_eq!(descriptors.len(), 2);
        assert_eq!(descriptors[0].name, "echo");
        assert_eq!(descriptors[1].name, "ping");
        assert!(
            descriptors[0]
                .input_schema
                .get("properties")
                .and_then(|p| p.get("text"))
                .is_some()
        );
    }

    #[test]
    fn tool_returning_invalid_arguments_surfaces_error() {
        let mut registry: ToolRegistry<()> = ToolRegistry::new();
        registry.register(Box::new(EchoTool { name: "echo" }));
        let tool = registry.get("echo").expect("present");
        let mut ctx = ();
        let err = tool.call(json!({}), &mut ctx).expect_err("missing text");
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    fn build_server_with_echo() -> Server<()> {
        let mut server = Server::new(ServerInfo::new("test-server", "0.0.1"));
        server
            .registry_mut()
            .register(Box::new(EchoTool { name: "echo" }));
        server
    }

    #[test]
    fn initialize_response_advertises_tools_capability_and_server_info() {
        let mut server = build_server_with_echo();
        let request: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
                .expect("parse");
        let response = server.handle(request, &mut ()).expect("response");
        let result = response.result.expect("success");
        assert_eq!(result["protocolVersion"], json!(PROTOCOL_VERSION));
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], json!("test-server"));
        assert_eq!(result["serverInfo"]["version"], json!("0.0.1"));
    }

    #[test]
    fn notifications_initialized_marks_server_initialized_and_returns_no_response() {
        let mut server = build_server_with_echo();
        assert!(!server.is_initialized());
        let request: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .expect("parse");
        let response = server.handle(request, &mut ());
        assert!(response.is_none());
        assert!(server.is_initialized());
    }

    #[test]
    fn tools_list_returns_descriptors_in_insertion_order() {
        let mut server = Server::new(ServerInfo::new("test", "0"));
        server
            .registry_mut()
            .register(Box::new(EchoTool { name: "alpha" }));
        server
            .registry_mut()
            .register(Box::new(EchoTool { name: "beta" }));
        let request: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
                .expect("parse");
        let response = server.handle(request, &mut ()).expect("response");
        let tools = response.result.expect("success")["tools"].clone();
        let names: Vec<_> = tools
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().expect("name").to_owned())
            .collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn tools_call_dispatches_to_the_named_tool() {
        let mut server = build_server_with_echo();
        let request: JsonRpcRequest = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}"#,
        )
        .expect("parse");
        let response = server.handle(request, &mut ()).expect("response");
        let result = response.result.expect("success");
        // isError is skipped on serialization when false; both shapes
        // are spec-compliant. Verify it's absent or explicitly false.
        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(!is_error);
        let content = &result["content"];
        assert_eq!(content[0]["type"], json!("text"));
        assert_eq!(content[0]["text"], json!("hi"));
    }

    #[test]
    fn tools_call_unknown_tool_returns_invalid_params() {
        let mut server = build_server_with_echo();
        let request: JsonRpcRequest = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
        )
        .expect("parse");
        let response = server.handle(request, &mut ()).expect("response");
        let error = response.error.expect("error");
        assert_eq!(error.code, error_code::INVALID_PARAMS);
        assert!(error.message.contains("nope"));
    }

    #[test]
    fn tools_call_with_failing_tool_returns_iserror_response_not_jsonrpc_error() {
        let mut server = build_server_with_echo();
        // Echo tool requires `text`; missing it triggers ToolError.
        let request: JsonRpcRequest = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"echo","arguments":{}}}"#,
        )
        .expect("parse");
        let response = server.handle(request, &mut ()).expect("response");
        let result = response.result.expect("not a JSON-RPC error");
        assert_eq!(result["isError"], json!(true));
        assert!(
            result["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("text must be a string")
        );
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let mut server = build_server_with_echo();
        let request: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":6,"method":"who/knows"}"#)
                .expect("parse");
        let response = server.handle(request, &mut ()).expect("response");
        let error = response.error.expect("error");
        assert_eq!(error.code, error_code::METHOD_NOT_FOUND);
    }

    #[test]
    fn serve_runs_a_full_handshake_through_buffered_io() {
        let mut server = build_server_with_echo();
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hello"}}}"#,
            "\n",
        );
        let mut output = Vec::new();
        serve(&mut server, &mut (), input.as_bytes(), &mut output)
            .expect("serve should drain the input cleanly");

        let lines: Vec<&str> = std::str::from_utf8(&output)
            .expect("utf-8 output")
            .lines()
            .collect();
        // initialize → tools/list → tools/call produce 3 response lines;
        // the notification produces nothing.
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("protocolVersion"));
        assert!(lines[1].contains(r#""tools":["#));
        assert!(lines[2].contains(r#""text":"hello""#));
        assert!(server.is_initialized());
    }

    #[test]
    fn serve_emits_parse_error_with_null_id_for_malformed_input() {
        let mut server = build_server_with_echo();
        let mut output = Vec::new();
        serve(&mut server, &mut (), "not json\n".as_bytes(), &mut output)
            .expect("serve should keep going past parse errors");
        let line = std::str::from_utf8(&output).expect("utf-8");
        assert!(line.contains(r#""id":null"#));
        assert!(line.contains(r#""code":-32700"#));
    }

    #[test]
    fn serve_skips_blank_lines_silently() {
        let mut server = build_server_with_echo();
        let mut output = Vec::new();
        serve(&mut server, &mut (), "\n   \n\n".as_bytes(), &mut output)
            .expect("blank input should be a no-op");
        assert!(output.is_empty());
    }

    #[test]
    fn jsonrpc_error_helpers_emit_spec_codes() {
        assert_eq!(JsonRpcError::parse_error("x").code, error_code::PARSE_ERROR);
        assert_eq!(
            JsonRpcError::invalid_request("x").code,
            error_code::INVALID_REQUEST
        );
        assert_eq!(
            JsonRpcError::method_not_found("x").code,
            error_code::METHOD_NOT_FOUND
        );
        assert_eq!(
            JsonRpcError::invalid_params("x").code,
            error_code::INVALID_PARAMS
        );
        assert_eq!(
            JsonRpcError::internal_error("x").code,
            error_code::INTERNAL_ERROR
        );
    }
}
