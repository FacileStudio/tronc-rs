//! A remote MCP server for a Facile app, a port of Go `tronc/mcp`: rmcp's Streamable HTTP
//! transport in stateless mode (spec 2026-07-28), behind access tokens bound to the server's
//! resource URI, with the RFC 9728 metadata, an Origin check, a write-scope gate and a
//! per-person rate limit.
//!
//! An app hands [`Server::new`] its curated tools, built with [`tool`], and the [`Verifier`] its
//! auth kit provides. tronc never depends on the kit. Tool names carry the app prefix, every tool
//! declares its annotations, and `tools/list` comes out sorted by name.
//!
//! Sensitive actions go through [`Confirmations`]: an MRTR `input_required` question when the
//! client can elicit, otherwise a pending action and a confirm link, with HMAC-signed state,
//! single-use claims and a changed-target guard. The pagination and truncation helpers keep
//! results under about 25k tokens, and [`failure`] turns service errors into ones a model can
//! act on.

mod audit;
mod auth;
mod call;
mod confirm_run;
mod confirmations;
mod failure;
mod gate;
mod handler;
mod options;
mod page;
mod pending;
mod postgres;
mod postgres_sql;
mod server;
mod signer;
mod tool;

pub use audit::{Event, audit};
pub use auth::{
    EXTRA_ACTOR, EXTRA_CLIENT_ID, ProtectedResourceMetadata, TokenInfo, Verifier, VerifyError, VerifyFuture,
    WELL_KNOWN, metadata_url,
};
pub use call::{Call, Caller, MRTR_VERSION};
pub use confirmations::{
    CHANGED, ConfirmConfig, Confirmations, DEFAULT_ASK_TTL, DEFAULT_PENDING_TTL, Kind, KindFuture, Outcome,
    STATE_CHANGED, STATE_MISSING, STATE_READY, STATUS_DECLINED, STATUS_DONE, STATUS_PENDING, changed, is_changed,
};
pub use failure::{ToolError, UNAVAILABLE, failure};
pub use gate::MAX_REQUEST_BODY_BYTES;
pub use options::{DEFAULT_LIST_TTL, Options, Scopes};
pub use page::{
    FORMAT_CONCISE, FORMAT_DETAILED, INVALID_CURSOR, RESULT_BUDGET, clip, decode_cursor, detailed, encode_cursor,
    encoded_size, fit_count,
};
pub use pending::{
    Action, GONE, OUTCOME_DECLINED, OUTCOME_DISMISSED, OUTCOME_DONE, OUTCOME_RUNNING, OUTCOME_STALE, Pending,
    PendingFuture, gone,
};
pub use postgres::{PENDING_SCHEMA, PostgresPending};
pub use rmcp;
pub use server::{Server, mount};
pub use signer::{CONFIRM_INPUT, INVALID_CONFIRMATION, invalid_confirmation};
pub use tool::{Annotations, Answer, Tool, ToolSpec, tool};

#[cfg(test)]
mod tests;
