//! The client a suite app uses to reach Bulle as the signed-in user, a port of Go `tronc/bulle`:
//! today the per-app Ideas API, later any AI call through [`Client::call`].
//!
//! Every AI feature in the suite goes through Bulle and needs a plan with `ai.access`. The
//! bearer token comes from a [`TokenSource`], which in practice is porte's token exchange
//! (audience `bulle`, scopes `bulle.read` and `bulle.write`); tronc never depends on porte.
//!
//! Failures come back typed: a 402 is [`Error::PlanRequired`], any other 4xx an
//! [`Error::Status`] that keeps its status (429 is the daily cap, 409 a batch already running),
//! and a 5xx, a timeout or a network error is [`Error::Unreachable`]. Paths are built from
//! escaped segments, so nothing a browser sends becomes a path as it is. [`mount_ideas`] serves
//! the `/ideas*` routes to the app's own frontend.

mod client;
mod error;
mod handlers;
mod ideas;
mod request;
mod router;
mod types;
mod wire;

pub use client::{Client, DEFAULT_TIMEOUT, MAX_ANSWER_BYTES};
pub use error::{Error, PlanRequired};
pub use request::{ForgetFuture, Request, TokenFuture, TokenSource};
pub use router::{AiAccessFn, IdeasOptions, IdeasPage, SpaceFn, TokensFn, mount_ideas};
pub use types::{
    Approval, Batch, Idea, IdeaList, Output, Run, RunStart, STATE_DONE, STATE_FAILED, STATE_NEEDS_APPROVAL,
    STATE_RUNNING, Source, run_state,
};
