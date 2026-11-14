#![cfg(feature = "mcp")]

use serde::{Deserialize, Serialize};
use tronc::Error;
use tronc::mcp::{
    Action, Annotations, INVALID_CURSOR, RESULT_BUDGET, UNAVAILABLE, clip, cursor, decode_cursor, detailed, digest,
    encode_cursor, encoded_size, failure, fit_count, offset, page, state_key, target_ids,
};

#[test]
fn failure_keeps_only_actionable_text() {
    assert_eq!(failure(Error::not_found("event not found"), "Call app_list.").0, "Event not found. Call app_list.");
    assert_eq!(failure(Error::not_found("event not found"), "").0, "Event not found.");
    assert_eq!(failure(Error::invalid("title is required"), "x").0, "Title is required");
    assert_eq!(failure(Error::internal("db down", "boom"), "x").0, UNAVAILABLE);
    assert_eq!(failure(Error::unavailable("bulle did not answer"), "x").0, "Bulle did not answer");
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

#[test]
fn target_helpers_match_go() {
    assert_eq!(digest(["a", "b"]), "59b271ae1bbcb1d31d41929817f4b16f", "Go mcp.Digest(\"a\", \"b\")");
    assert_ne!(digest(["ab"]), digest(["a", "b"]), "fields are separated");
    let key = state_key("docs", "secret");
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex, "efab34c626a166c0400b1b86a0302e4b0671dbbfb0b04cb9ac905c9ef29caf5b",
        "sha256(docs-mcp-state:secret)"
    );
    let action = |user: &str, target: &str| Action { user_id: user.into(), target: target.into(), ..blank() };
    assert_eq!(target_ids(&action("7", "42"), Error::invalid("bad")).expect("ids"), (7, 42));
    assert_eq!(target_ids(&action("7", "x"), Error::invalid("bad")).expect_err("not an id").message, "bad");
}

fn blank() -> Action {
    let now = time::OffsetDateTime::UNIX_EPOCH;
    Action {
        id: String::new(),
        user_id: String::new(),
        kind: String::new(),
        target: String::new(),
        digest: String::new(),
        summary: String::new(),
        client_id: String::new(),
        created_at: now,
        expires_at: now,
    }
}

#[test]
fn pages_move_by_offset_and_fit_the_budget() {
    let items: Vec<u32> = (0..5).collect();
    let (first, next) = page(&items, "", 2).expect("first");
    assert_eq!(first, [0, 1]);
    let (second, next) = page(&items, &next, 2).expect("second");
    let (last, end) = page(&items, &next, 2).expect("last");
    assert_eq!((second, last, end.as_str()), (&[2, 3][..], &[4][..], ""));
    assert_eq!(page(&items, &encode_cursor(&9usize), 2).expect("past the end clamps").0, &[] as &[u32]);
    assert_eq!(page(&items, "", 0).expect("at least one").0, [0]);
    assert_eq!(page(&items, "!!", 2).expect_err("bad").0, INVALID_CURSOR);
    assert_eq!((offset(&cursor(3, 5)).expect("offset"), cursor(5, 5).as_str()), (3, ""));
}

#[test]
fn presets_follow_the_annotations_rule() {
    let hints = |a: Annotations| (a.read_only, a.destructive, a.idempotent);
    assert_eq!(hints(Annotations::READ), (true, false, true));
    assert_eq!(hints(Annotations::ADD), (false, false, false));
    assert_eq!(hints(Annotations::SET), (false, false, true));
    assert_eq!(
        (hints(Annotations::OVERWRITE), hints(Annotations::REMOVE)),
        ((false, true, true), (false, true, false))
    );
    assert!(Annotations::READ.open_world().open_world && Annotations::READ.open_world().read_only);
    assert_eq!(failure(Error::failed("échec du calcul"), "").0, "Échec du calcul", "the first letter, not byte");
    assert_eq!(failure(Error::failed("ßig"), "").0, "ßig", "one letter stays one letter, as Go ToUpper");
}
