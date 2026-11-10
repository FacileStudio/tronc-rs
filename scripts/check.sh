#!/usr/bin/env sh
#
# The repository quality gate. Reports, never rewrites.
#
#   sh scripts/check.sh
#
# Runs rustfmt, clippy and the tests with every feature on, then filet. The database tests run
# when TEST_DATABASE_URL is set and announce a skip otherwise (see docs/development.md).
# Deliberately not invoked through mise, for the same reason as Go tronc: a broken tool in a
# global mise config must not take this gate down.

set -eu

cd "$(git rev-parse --show-toplevel)"

cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
if command -v filet >/dev/null 2>&1; then
  filet check .
else
  echo "check: no 'filet' on PATH, skipping the filet pass (CI still runs it)" >&2
fi
