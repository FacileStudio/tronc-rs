use std::collections::HashMap;

use http::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::Client;
use super::error::Error;
use super::request::{Request, TokenSource};

/// One completion an app asks for its user, the body of Bulle's `POST /api/ai/complete`.
/// `input` is untrusted data the prompt works on, which Bulle fences off from the instructions.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CompleteRequest {
    /// Extra system instructions, at most 4 KiB.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub system: String,
    /// The instructions, at most 8 KiB.
    pub prompt: String,
    /// The material the prompt works on, at most 64 KiB.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub input: String,
    /// A JSON Schema that turns the answer into a validated JSON value instead of text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    /// The output budget. Zero lets Bulle pick.
    #[serde(skip_serializing_if = "is_zero")]
    pub max_tokens: u32,
    /// The space the request is for. Bulle grants AI when the person's own plan or this space's
    /// plan has `ai.access`. Empty means the person's own plan.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub space_id: String,
}

/// Bulle's answer to a completion: `text`, or `json` when the request had a schema.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Completion {
    /// The answer as text.
    pub text: String,
    /// The answer as JSON, validated against the request's schema.
    pub json: Option<Value>,
    /// The model that wrote it.
    pub model: String,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl Client {
    /// Asks Bulle for one completion as the user behind `tokens`. A 402 is
    /// [`Error::PlanRequired`]; a 429 is the user's daily cap.
    pub async fn complete<S: TokenSource + ?Sized>(
        &self,
        tokens: &S,
        request: &CompleteRequest,
    ) -> Result<Completion, Error> {
        let body =
            serde_json::to_value(request).map_err(|e| Error::Unreachable(format!("unencodable request: {e}")))?;
        self.call(tokens, Request::new(Method::POST, ["ai", "complete"]).json(body)).await
    }
}

/// The header that names the user a [`Client::decide_ai`] call acts for, as Go
/// `bulle.ActingForHeader`.
pub const ACTING_FOR_HEADER: &str = "X-Bulle-Acting-For";

/// One question of a decision, as Bulle's `jev.Question`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Question {
    /// The question type, such as `choice`.
    #[serde(rename = "type")]
    pub kind: String,
    /// What the model decides.
    pub instructions: Value,
    /// How the model judges, when the type takes criteria.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Value>,
    /// The answers it picks from.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// A decision an app asks of Bulle's `POST /api/ai/decide`, as Go `bulle.DecideRequest`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DecideRequest {
    /// The email of the user the decision is for, sent as [`ACTING_FOR_HEADER`]. Required.
    #[serde(skip)]
    pub acting_for: String,
    /// The state the questions are about.
    pub state: Value,
    /// The questions, by name.
    pub questions: HashMap<String, Question>,
}

/// One answer of a [`Decision`].
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AiAnswer {
    /// The option picked.
    pub choice: String,
    /// How sure the model is.
    pub confidence: f64,
    /// The weight of each option.
    pub probabilities: HashMap<String, f64>,
}

/// What a decision cost.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Usage {
    /// Prompt tokens.
    pub input_tokens: i64,
    /// Answer tokens.
    pub output_tokens: i64,
    /// The cost, when Bulle reports one.
    pub cost: f64,
}

/// Bulle's answer to a [`DecideRequest`], as Go `bulle.Decision`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Decision {
    /// The answers, by question name.
    pub answers: HashMap<String, AiAnswer>,
    /// The model that decided.
    pub model: String,
    /// What it cost.
    pub usage: Usage,
}

impl Client {
    /// Asks Bulle for a decision for the user named by `request.acting_for`, as Go `DecideAI`.
    /// Without that email nothing is sent and the call fails with [`Error::NoActingFor`].
    pub async fn decide_ai<S: TokenSource + ?Sized>(
        &self,
        tokens: &S,
        request: &DecideRequest,
    ) -> Result<Decision, Error> {
        let acting_for = request.acting_for.trim();
        if acting_for.is_empty() {
            return Err(Error::NoActingFor);
        }
        let body =
            serde_json::to_value(request).map_err(|e| Error::Unreachable(format!("unencodable request: {e}")))?;
        let call = Request::new(Method::POST, ["ai", "decide"]).json(body).header(ACTING_FOR_HEADER, acting_for);
        self.call(tokens, call).await
    }
}
