use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rmcp::model::{CallToolRequestParams, CallToolResponse, ElicitationAction, InputResponses, ProtocolVersion};
use serde::Deserialize;

use super::client::{ClientOptions, connect};
use super::results::{answering, call, call_once, decode, text};
use super::{Config, ConfirmCase, Target};
use crate::mcp::{CONFIRM_INPUT, STATUS_DECLINED, STATUS_PENDING};

#[derive(Debug, Deserialize)]
struct Status {
    #[serde(default)]
    status: String,
    #[serde(default)]
    confirm_url: String,
}

fn accept() -> InputResponses {
    let answer = serde_json::json!({ "action": "accept", "content": { CONFIRM_INPUT: true } });
    BTreeMap::from([(CONFIRM_INPUT.to_owned(), answer)])
}

fn retry(tool: &str, target: &Target, state: String) -> CallToolRequestParams {
    let mut params = CallToolRequestParams::new(tool.to_owned()).with_arguments(target.args.clone());
    params.request_state = Some(state);
    params.input_responses = Some(accept());
    params
}

/// Runs the confirmation path of one sensitive tool: a client that cannot elicit and a
/// pre-MRTR client get a link and nothing runs, a decline runs nothing, an accept runs once and
/// its state cannot be replayed, and a link id cannot be passed off as a signed state.
pub(super) async fn check(config: &Config, case: &ConfirmCase) {
    check_link(config, case, ClientOptions::default()).await;
    let asked = Arc::new(AtomicUsize::new(0));
    let legacy = ClientOptions {
        elicit: Some(answering(ElicitationAction::Accept, asked.clone())),
        version: Some(ProtocolVersion::V_2025_11_25),
    };
    check_link(config, case, legacy).await;
    assert_eq!(asked.load(Ordering::SeqCst), 0, "{}: a stateless server cannot ask a pre-MRTR client", case.tool);
    check_decline(config, case).await;
    check_accept(config, case).await;
    check_forged(config, case).await;
}

async fn check_link(config: &Config, case: &ConfirmCase, options: ClientOptions) {
    let target = (case.setup)().await;
    let client = connect(&config.url, &(config.token)(&[]), options).await;
    let out: Status = decode(&call(&client, &case.tool, target.args.clone()).await);
    let runs = (target.runs)().await;
    assert!(
        out.status == STATUS_PENDING && out.confirm_url.contains("action=") && runs == 0,
        "{}: want {STATUS_PENDING} with a confirm_url and no run, got {out:?} and {runs} runs",
        case.tool
    );
}

async fn check_decline(config: &Config, case: &ConfirmCase) {
    let target = (case.setup)().await;
    let asked = Arc::new(AtomicUsize::new(0));
    let options = ClientOptions { elicit: Some(answering(ElicitationAction::Decline, asked.clone())), version: None };
    let client = connect(&config.url, &(config.token)(&[]), options).await;
    let out: Status = decode(&call(&client, &case.tool, target.args.clone()).await);
    let (asked, runs) = (asked.load(Ordering::SeqCst), (target.runs)().await);
    assert!(
        out.status == STATUS_DECLINED && asked == 1 && runs == 0,
        "{}: want one question, status declined and no run, got {asked} questions, {out:?}, {runs} runs",
        case.tool
    );
}

async fn check_accept(config: &Config, case: &ConfirmCase) {
    let target = (case.setup)().await;
    let options = ClientOptions { elicit: Some(answering(ElicitationAction::Accept, Arc::default())), version: None };
    let client = connect(&config.url, &(config.token)(&[]), options).await;
    let first = CallToolRequestParams::new(case.tool.clone()).with_arguments(target.args.clone());
    let state = match call_once(&client, first).await {
        CallToolResponse::InputRequired(question) => question.request_state.unwrap_or_default(),
        other => panic!("{}: a client that can elicit must get input_required, got {other:?}", case.tool),
    };
    assert!(!state.is_empty(), "{}: input_required must carry a request state", case.tool);
    let done = call_once(&client, retry(&case.tool, &target, state.clone())).await;
    let runs = (target.runs)().await;
    assert!(
        matches!(&done, CallToolResponse::Complete(r) if r.is_error != Some(true)) && runs == 1,
        "{}: an accepted confirmation must run once, got {done:?}, {runs} runs",
        case.tool
    );
    let replay = call_once(&client, retry(&case.tool, &target, state)).await;
    let runs = (target.runs)().await;
    assert!(
        matches!(&replay, CallToolResponse::Complete(r) if r.is_error == Some(true)) && runs == 1,
        "{}: a replayed confirmation must fail and run nothing, got {replay:?}, {runs} runs",
        case.tool
    );
}

async fn check_forged(config: &Config, case: &ConfirmCase) {
    let target = (case.setup)().await;
    let client = connect(&config.url, &(config.token)(&[]), ClientOptions::default()).await;
    let out: Status = decode(&call(&client, &case.tool, target.args.clone()).await);
    let id = out.confirm_url.rsplit('=').next().unwrap_or("").to_owned();
    let forged = call_once(&client, retry(&case.tool, &target, id)).await;
    let runs = (target.runs)().await;
    let refused = matches!(&forged, CallToolResponse::Complete(r) if r.is_error == Some(true));
    let detail = match &forged {
        CallToolResponse::Complete(result) => text(result),
        other => format!("{other:?}"),
    };
    assert!(refused && runs == 0, "{}: a forged state must fail and run nothing, got {detail}, {runs} runs", case.tool);
}
