#![cfg(feature = "mcp")]

use serde::{Deserialize, Serialize};
use tronc::Error;
use tronc::mcp::{
    INVALID_CURSOR, RESULT_BUDGET, UNAVAILABLE, clip, decode_cursor, detailed, encode_cursor, encoded_size, failure,
    fit_count,
};

#[test]
fn failure_keeps_only_actionable_text() {
    assert_eq!(failure(Error::not_found("event not found"), "Call app_list.").0, "Event not found. Call app_list.");
    assert_eq!(failure(Error::not_found("event not found"), "").0, "Event not found.");
    assert_eq!(failure(Error::invalid("title is required"), "x").0, "Title is required");
    assert_eq!(failure(Error::internal("db down", "boom"), "x").0, UNAVAILABLE);
    assert_eq!(failure(Error::unavailable("starting"), "x").0, UNAVAILABLE);
    let quota = Error::payment_required("you used today's searches").with_upgrade_url("https://sso.example/billing");
    assert_eq!(failure(quota, "x").0, "You used today's searches. Upgrade at https://sso.example/billing");
    assert_eq!(failure(Error::payment_required("upgrade."), "x").0, "Upgrade.");
    let from: tronc::mcp::ToolError = Error::internal("secret detail", "cause").into();
    assert_eq!(from.0, UNAVAILABLE, "the ? conversion hides internal detail");
}

#[test]
fn clip_cuts_on_a_character_boundary() {
    assert_eq!(clip("héllo", 2), ("h", true));
    assert_eq!(clip("hello", 5), ("hello", false));
    assert_eq!(clip("ab cd", 3), ("ab", true));
    assert_eq!(clip("é", 0), ("", true));
}

#[test]
fn fit_count_finds_the_largest_prefix_under_budget() {
    assert_eq!(fit_count(1000, |n| n * 100), RESULT_BUDGET / 100);
    assert_eq!(fit_count(10, |n| n * 100), 10);
    assert_eq!(fit_count(10, |n| RESULT_BUDGET + n), 0);
    assert_eq!(encoded_size(&["ab"]), 6);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Position {
    offset: u32,
}

#[test]
fn cursors_round_trip_and_bad_ones_are_refused() {
    let cursor = encode_cursor(&Position { offset: 40 });
    assert_eq!(decode_cursor::<Position>(&cursor).expect("cursor"), Some(Position { offset: 40 }));
    assert_eq!(decode_cursor::<Position>("").expect("empty"), None);
    assert_eq!(decode_cursor::<Position>("!!").expect_err("bad").0, INVALID_CURSOR);
    assert_eq!(detailed(""), Ok(false));
    assert_eq!(detailed("detailed"), Ok(true));
    assert!(detailed("verbose").is_err());
}
