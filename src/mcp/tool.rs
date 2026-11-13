use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rmcp::handler::server::common::{schema_for_output, schema_for_type};
use rmcp::model::{CallToolResponse, CallToolResult, InputRequiredResult, JsonObject, ToolAnnotations};
use rmcp::schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::call::Call;
use super::failure::ToolError;

/// A tool's hints for the client's permission UI. Every tool declares all four, because clients
/// treat a missing destructive or open-world hint as true. They are UX, never the gate: the
/// scopes and [`super::Confirmations`] are.
///
/// The suite rule: any tool that overwrites or removes user data, or silences an alert, is
/// `destructive`, a rename included. Only a tool whose repeat call with the same arguments has
/// no further effect is `idempotent`. Start from a preset: [`Self::READ`], [`Self::ADD`],
/// [`Self::SET`], [`Self::OVERWRITE`] or [`Self::REMOVE`], plus [`Self::open_world`] for content
/// from outside the app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Annotations {
    /// Changes nothing. Every other tool needs the write scope.
    pub read_only: bool,
    /// May delete or overwrite. A read-only tool is never destructive.
    pub destructive: bool,
    /// Has no further effect when called again with the same arguments.
    pub idempotent: bool,
    /// Reads or reaches content from outside the app, such as mail from other senders.
    pub open_world: bool,
}

impl Annotations {
    /// Reads only.
    pub const READ: Self = Self { read_only: true, destructive: false, idempotent: true, open_world: false };
    /// Creates something new on every call, such as a comment or an event.
    pub const ADD: Self = Self { read_only: false, destructive: false, idempotent: false, open_world: false };
    /// Sets a state without losing data, so a repeat changes nothing, such as moving a card or
    /// restoring from the trash.
    pub const SET: Self = Self { read_only: false, destructive: false, idempotent: true, open_world: false };
    /// Replaces user data with the given value, such as a rename or an edit.
    pub const OVERWRITE: Self = Self { read_only: false, destructive: true, idempotent: true, open_world: false };
    /// Deletes, trashes or silences, such as deleting a board or muting an alert.
    pub const REMOVE: Self = Self { read_only: false, destructive: true, idempotent: true, open_world: false };

    /// The same hints for a tool that reads or reaches content from outside the app.
    pub const fn open_world(self) -> Self {
        Self { open_world: true, ..self }
    }
}

/// Describes one tool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolSpec {
    /// `<app>_<verb>` or `<app>_<resource>_<verb>`, lowercase.
    pub name: String,
    /// The human name shown in clients.
    pub title: String,
    /// Tells the model when to call the tool and what it returns.
    pub description: String,
    /// Required: `None` is refused at boot, so each tool states its hints on purpose.
    pub annotations: Option<Annotations>,
}

/// What a tool handler answers: its output, or an MRTR question for the client to put to the
/// person before it retries.
#[derive(Debug, Clone)]
pub enum Answer<Out> {
    /// The finished result, sent as structured content and as its JSON text.
    Output(Out),
    /// An `input_required` result, such as the one [`super::Confirmations::request`] makes.
    InputRequired(InputRequiredResult),
}

impl<Out> From<Out> for Answer<Out> {
    fn from(out: Out) -> Self {
        Self::Output(out)
    }
}

pub(super) type Reply = Pin<Box<dyn Future<Output = Result<CallToolResponse, ToolError>> + Send>>;
type Handler = Arc<dyn Fn(Call, JsonObject) -> Reply + Send + Sync>;

/// A curated tool, ready for [`super::Server::new`].
#[derive(Clone)]
pub struct Tool {
    pub(super) spec: ToolSpec,
    pub(super) input: Arc<JsonObject>,
    pub(super) output: Arc<JsonObject>,
    pub(super) handler: Handler,
}

/// Builds a tool from `spec` and a typed handler. The input and output schemas come from `In`
/// and `Out`. The server checks the spec at boot; the handler runs only for a caller with a
/// verified token, and for a write tool only when the token holds the write scope.
pub fn tool<In, Out, F, Fut>(spec: ToolSpec, handler: F) -> Tool
where
    In: DeserializeOwned + JsonSchema + 'static,
    Out: Serialize + JsonSchema + 'static,
    F: Fn(Call, In) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Answer<Out>, ToolError>> + Send + 'static,
{
    let handler = Arc::new(handler);
    let erased: Handler = Arc::new(move |call: Call, arguments: JsonObject| {
        let handler = handler.clone();
        Box::pin(async move {
            let input = serde_json::from_value::<In>(serde_json::Value::Object(arguments))
                .map_err(|error| ToolError::new(format!("invalid arguments: {error}")))?;
            match handler(call, input).await? {
                Answer::Output(out) => Ok(CallToolResponse::Complete(structured(&out)?)),
                Answer::InputRequired(question) => Ok(CallToolResponse::InputRequired(question)),
            }
        })
    });
    Tool { spec, input: schema_for_type::<In>(), output: schema_for_output::<Out>(), handler: erased }
}

fn structured<Out: Serialize>(out: &Out) -> Result<CallToolResult, ToolError> {
    let value = serde_json::to_value(out).map_err(|_| ToolError::new(super::failure::UNAVAILABLE))?;
    Ok(CallToolResult::structured(value))
}

impl Tool {
    pub(super) fn definition(&self) -> rmcp::model::Tool {
        let a = self.spec.annotations.unwrap_or_default();
        let hints = ToolAnnotations::with_title(self.spec.title.clone())
            .read_only(a.read_only)
            .destructive(a.destructive)
            .idempotent(a.idempotent)
            .open_world(a.open_world);
        rmcp::model::Tool::new(self.spec.name.clone(), self.spec.description.clone(), self.input.clone())
            .with_title(self.spec.title.clone())
            .with_raw_output_schema(self.output.clone())
            .with_annotations(hints)
    }
}

/// Refuses a tool without the app prefix, a lowercase name, a title, a description and
/// annotations, or with a name already taken.
pub(super) fn check(spec: &ToolSpec, prefix: &str, seen: &mut HashSet<String>) -> Result<(), String> {
    let name = &spec.name;
    let rest = name.strip_prefix(prefix).unwrap_or("");
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(format!("mcp: tool {name:?} must be named {prefix}<verb>, in lowercase letters, digits and _"));
    }
    if seen.contains(name) {
        return Err(format!("mcp: tool {name:?} is registered twice"));
    }
    if spec.title.is_empty() || spec.description.is_empty() {
        return Err(format!("mcp: tool {name:?} needs a title and a description"));
    }
    match spec.annotations {
        None => Err(format!("mcp: tool {name:?} needs its annotations")),
        Some(a) if a.read_only && a.destructive => {
            Err(format!("mcp: tool {name:?} cannot be read-only and destructive"))
        }
        Some(_) => {
            seen.insert(name.clone());
            Ok(())
        }
    }
}
