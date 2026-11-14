#!/usr/bin/env sh
#
# Copies Go tronc's conformance vectors into tests/vectors and records the tag they came from.
#
#   sh scripts/sync-vectors.sh v0.23.0                                 clone FacileStudio/tronc at a tag
#   sh scripts/sync-vectors.sh ~/Code/Facile/tronc-wt-vectors v0.23.0  read a checkout of that tag
#   sh scripts/sync-vectors.sh path/to/conformance/vectors v0.23.0     read a vectors directory
#
# tests/vectors/SOURCE gets the tag, "local" when a directory is read without one. Each file
# keeps its own "source": Go stamps a subject with the release that last changed it. The whole
# directory is replaced, so a vector file Go adds or drops shows up as a new or deleted file. CI
# re-runs this against the recorded tag and fails on any difference, new files included:
#
#   sh scripts/sync-vectors.sh "$(cat tests/vectors/SOURCE)"
#   git add --intent-to-add tests/vectors && git diff --exit-code tests/vectors

set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <tag> | $0 <checkout|vectors-dir> [tag]" >&2
  exit 2
fi

root="$(git rev-parse --show-toplevel)"
target="$root/tests/vectors"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

tag="${2:-local}"
if [ -d "$1/conformance/vectors" ]; then
  from="$1/conformance/vectors"
elif [ -d "$1" ]; then
  from="$1"
else
  tag="$1"
  git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$tag" https://github.com/FacileStudio/tronc "$scratch/tronc"
  from="$scratch/tronc/conformance/vectors"
fi

set -- "$from"/*.json
if [ ! -f "$1" ]; then
  echo "sync-vectors: no vector file in $from" >&2
  exit 1
fi

mkdir -p "$target"
rm -f "$target"/*.json
cp "$@" "$target/"
echo "$tag" > "$target/SOURCE"
echo "sync-vectors: $# files from $from ($tag)"
