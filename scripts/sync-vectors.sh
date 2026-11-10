#!/usr/bin/env sh
#
# Copies Go tronc's conformance vectors into tests/vectors and records where they came from.
#
#   sh scripts/sync-vectors.sh v0.20.1                         clone FacileStudio/tronc at a tag
#   sh scripts/sync-vectors.sh ~/Code/Facile/tronc-wt-vectors  read a local checkout
#   sh scripts/sync-vectors.sh path/to/conformance/vectors     read a vectors directory
#
# tests/vectors/SOURCE gets the "source" field of the copied envelopes, so it always names the
# tag the Go test generated them from. CI re-runs this against that tag and fails on a diff:
#
#   sh scripts/sync-vectors.sh "$(sed 's/.*@//' tests/vectors/SOURCE)" && git diff --exit-code tests/vectors

set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <tag|checkout|vectors-dir>" >&2
  exit 2
fi

root="$(git rev-parse --show-toplevel)"
target="$root/tests/vectors"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

if [ -d "$1/conformance/vectors" ]; then
  from="$1/conformance/vectors"
elif [ -d "$1" ]; then
  from="$1"
else
  git clone --quiet --depth 1 --branch "$1" https://github.com/FacileStudio/tronc "$scratch/tronc"
  from="$scratch/tronc/conformance/vectors"
fi

set -- "$from"/*.json
if [ ! -f "$1" ]; then
  echo "sync-vectors: no vector file in $from" >&2
  exit 1
fi

source="$(sed -n 's/.*"source": *"\([^"]*\)".*/\1/p' "$1" | head -n 1)"
for file in "$@"; do
  if ! grep -q "\"source\": *\"$source\"" "$file"; then
    echo "sync-vectors: $file was not generated from $source" >&2
    exit 1
  fi
done

mkdir -p "$target"
rm -f "$target"/*.json
cp "$@" "$target/"
echo "$source" > "$target/SOURCE"
echo "sync-vectors: $# files from $source"
