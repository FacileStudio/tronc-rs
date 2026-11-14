use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use http::request::Parts;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tronc::Error;
use tronc::mcp::{Action, EXTRA_CLIENT_ID, TokenInfo, Verifier, VerifyError, VerifyFuture};

pub struct Tokens;

impl Verifier for Tokens {
    fn verify<'a>(&'a self, token: &'a str, _request: &'a Parts) -> VerifyFuture<'a> {
        Box::pin(async move {
            let (user, scopes) =
                token.split_once('|').filter(|(user, _)| !user.is_empty()).ok_or(VerifyError::Invalid)?;
            Ok(TokenInfo {
                user_id: user.to_owned(),
                scopes: scopes.split(',').map(str::to_owned).collect(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
                extra: HashMap::from([(EXTRA_CLIENT_ID.to_owned(), "test-client".to_owned())]),
            })
        })
    }
}

#[derive(Default)]
pub struct Notes {
    pub text: Mutex<HashMap<String, String>>,
    pub deleted: Mutex<HashMap<String, usize>>,
}

impl Notes {
    pub fn set(&self, id: &str, text: &str) {
        self.text.lock().expect("lock").insert(id.to_owned(), text.to_owned());
    }

    pub fn deletes(&self, id: &str) -> usize {
        self.deleted.lock().expect("lock").get(id).copied().unwrap_or(0)
    }

    pub fn fingerprint(&self, action: &Action) -> Result<String, Error> {
        self.text.lock().expect("lock").get(&action.target).cloned().ok_or_else(|| Error::not_found("note not found"))
    }

    pub fn remove(&self, action: &Action) {
        self.text.lock().expect("lock").remove(&action.target);
        *self.deleted.lock().expect("lock").entry(action.target.clone()).or_default() += 1;
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteRef {
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteInput {
    pub id: String,
    pub text: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct Nothing {}

#[derive(Serialize, JsonSchema)]
pub struct NoteList {
    pub ids: Vec<String>,
}
