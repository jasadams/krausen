#!/bin/sh
# Self-test for refresh-changed-mtimes.sh, in a temporary tree.
# Run: sh scripts/refresh-changed-mtimes-test.sh
set -eu
script="$(cd "$(dirname "$0")" && pwd)/refresh-changed-mtimes.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tree="$work/tree"
marker="$work/marker"
manifest="$tree/target/manifest"
mkdir -p "$tree/src" "$tree/target" "$tree/node_modules/x"
cd "$tree"
echo a > src/a; echo b > src/b; echo n > node_modules/x/n
touch -t 200001020000 "$marker"   # newer than the stamped time, older than now
fail=0
check() {
  if [ "$2" = "$3" ]; then echo "PASS: $1"; else echo "FAIL: $1 (expected '$2', got '$3')"; fail=1; fi
}
# Give every file in the tree an old mtime, as COPY would for edits older than the build.
stamp_all() { find . -type f -not -path './target/*' -exec touch -t 200001010000 {} +; }
# Paths of files the script touched (newer than the marker), one per line, sorted.
touched() { find . -type f -newer "$marker" -not -path './target/*' | sort; }
run() { sh "$script" refresh "$tree" "$manifest" > /dev/null; }
install() { sh "$script" commit "$manifest"; }

stamp_all
run
check "no manifest: every source file touched, node_modules not" "./src/a
./src/b" "$(touched)"
check "refresh does not install the new manifest" "no" "$([ -f "$manifest" ] && echo yes || echo no)"
install

stamp_all; run
check "unchanged content: no file touched" "" "$(touched)"

echo changed > src/a; stamp_all; run
check "changed content with an old mtime is touched" "./src/a" "$(touched)"

# A failed build never installs the manifest, so the retry touches the file again.
stamp_all; run
check "failed build, manifest not installed: retry touches it again" "./src/a" "$(touched)"
install

echo new > "src/c d"; stamp_all; run
check "new file whose name has a space is touched" "./src/c d" "$(touched)"
install

rm src/b; stamp_all; run
check "deleted file: nothing else touched" "" "$(touched)"
install

# Edit, the build compiles it, a later step fails, the edit is reverted, rebuild.
# The cache holds output built from the failed attempt's content, so the reverted
# file must be touched again although it matches the installed manifest.
echo edited > src/a; stamp_all; run              # failed build: no install
echo changed > src/a; stamp_all; run             # content back to the manifest's
check "edit, failed build, revert: the file is still touched" "./src/a" "$(touched)"
stamp_all; run                                   # a second failed build
check "pending survives a second failed build" "./src/a" "$(touched)"
install
stamp_all; run
check "after a successful build nothing is pending" "" "$(touched)"
exit "$fail"
