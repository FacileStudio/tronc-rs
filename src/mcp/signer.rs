use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use rmcp::model::{
    ElicitRequest, ElicitRequestParams, ElicitResult, ElicitationAction, ElicitationSchema, InputRequest,
    InputRequiredResult,
};
use sha2::Sha256;
use time::OffsetDateTime;

use super::call::Call;
use crate::errors::Error;

/// The key of the one input request a confirmation makes.
pub const CONFIRM_INPUT: &str = "confirm";

/// The refusal for a confirmation state this server did not sign, signed for someone else, for
/// another tool, or too long ago.
pub const INVALID_CONFIRMATION: &str =
    "this confirmation is no longer valid. Call the tool again to ask the user afresh";

/// The `failed_precondition` error carrying [`INVALID_CONFIRMATION`].
pub fn invalid_confirmation() -> Error {
    Error::failed(INVALID_CONFIRMATION)
}

static CONFIRM_SCHEMA: LazyLock<ElicitationSchema> = LazyLock::new(|| {
    ElicitationSchema::builder()
        .required_bool_with(CONFIRM_INPUT, |schema| schema.title("Confirm").with_default(true))
        .build_unchecked()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    Accepted,
    Declined,
}

/// Makes and checks the request state of a confirmation: the pending action, the person and an
/// expiry, signed with HMAC-SHA256, so a client cannot forge a confirmation for an action the
/// server did not ask about.
#[derive(Clone)]
pub(super) struct Signer {
    pub key: Vec<u8>,
    pub ttl: Duration,
}

impl Signer {
    pub fn ask(&self, user_id: &str, id: &str, message: &str) -> InputRequiredResult {
        let params = ElicitRequestParams::FormElicitationParams {
            meta: None,
            message: message.to_owned(),
            requested_schema: CONFIRM_SCHEMA.clone(),
        };
        let requests =
            BTreeMap::from([(CONFIRM_INPUT.to_owned(), InputRequest::Elicitation(ElicitRequest::new(params)))]);
        let expires = OffsetDateTime::now_utc() + self.ttl;
        InputRequiredResult::new(Some(requests), Some(self.sign(user_id, id, expires.unix_timestamp())))
    }

    pub fn answer(&self, call: &Call) -> Result<(String, Verdict), Error> {
        let state = call.request_state.as_deref().unwrap_or("");
        let id = self.verify(state, &call.caller.user_id, OffsetDateTime::now_utc().unix_timestamp())?;
        let answer = call.input_responses.get(CONFIRM_INPUT).cloned().ok_or_else(invalid_confirmation)?;
        let result: ElicitResult = serde_json::from_value(answer).map_err(|_| invalid_confirmation())?;
        let unticked =
            result.content.as_ref().and_then(|c| c.get(CONFIRM_INPUT)) == Some(&serde_json::Value::Bool(false));
        let verdict =
            if result.action != ElicitationAction::Accept || unticked { Verdict::Declined } else { Verdict::Accepted };
        Ok((id, verdict))
    }

    pub fn sign(&self, user_id: &str, id: &str, expires: i64) -> String {
        let payload = format!("{user_id}.{id}.{expires}");
        let mac = self.mac(&payload);
        format!("{payload}.{mac}")
    }

    pub fn verify(&self, state: &str, user_id: &str, now: i64) -> Result<String, Error> {
        let parts: Vec<&str> = state.split('.').collect();
        let [user, id, expires, mac] = parts.as_slice() else {
            return Err(invalid_confirmation());
        };
        if user_id.is_empty() || *user != user_id {
            return Err(invalid_confirmation());
        }
        let payload = format!("{user}.{id}.{expires}");
        let mut check = self.hmac();
        check.update(payload.as_bytes());
        let mac = decode_hex(mac).ok_or_else(invalid_confirmation)?;
        check.verify_slice(&mac).map_err(|_| invalid_confirmation())?;
        match expires.parse::<i64>() {
            Ok(expires) if now <= expires => Ok((*id).to_owned()),
            _ => Err(invalid_confirmation()),
        }
    }

    fn hmac(&self) -> Hmac<Sha256> {
        <Hmac<Sha256> as KeyInit>::new_from_slice(&self.key).unwrap_or_else(|_| unreachable!("HMAC takes any key"))
    }

    fn mac(&self, payload: &str) -> String {
        let mut mac = self.hmac();
        mac.update(payload.as_bytes());
        mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}
