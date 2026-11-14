use serde_json::Value;
use tronc::mcp::{PlanLimit, digest, failure, plan_failure, state_key};
use tronc::{Code, Error};

use super::assert::expect_eq;
use super::support::{Case, text};

pub fn digest_case(case: &Case) -> Result<(), String> {
    let fields: Vec<String> = serde_json::from_value(case.input["fields"].clone()).map_err(|e| e.to_string())?;
    expect_eq("digest", digest(&fields).as_str(), text(&case.want, "digest"))
}

pub fn state_key_case(case: &Case) -> Result<(), String> {
    let key = state_key(text(&case.input, "app"), text(&case.input, "secret"));
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    expect_eq("key", hex.as_str(), text(&case.want, "key_hex"))
}

struct Limit(String, String);

impl PlanLimit for Limit {
    fn plan_limit(&self) -> (String, String) {
        (self.0.clone(), self.1.clone())
    }
}

pub fn failure_case(case: &Case) -> Result<(), String> {
    let input = &case.input;
    let (message, hint, link) = (text(input, "message"), text(input, "hint"), text(input, "upgrade_url"));
    let got = if input.get("plan_limit_feature").is_some() {
        plan_failure(&Limit(text(input, "plan_limit_feature").to_owned(), link.to_owned()))
    } else if input.get("raw").and_then(Value::as_bool) == Some(true) {
        failure(Error::internal("raw", message.to_owned()), hint)
    } else {
        let code = Code::parse(text(input, "code")).ok_or("unknown code")?;
        let mut error = Error::new(code, message);
        error.upgrade_url = Some(link.to_owned()).filter(|l| !l.is_empty());
        failure(error, hint)
    };
    expect_eq("text", got.0.as_str(), text(&case.want, "text"))
}
