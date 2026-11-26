#!/bin/sh
# Make cargo's mtime-based freshness agree with the content of the build context.
#
# Cargo calls a workspace crate fresh when none of its source files is newer than the
# crate's cached dep-info file. `COPY . .` keeps the source files' original mtimes, so a
# file whose mtime is older than the cached build output (an edit made while an earlier
# build ran, a `git checkout` of older files, `touch -t`) looks unchanged and the image
# keeps the old code.
#
# The manifest (inside the target cache mount, next to the compiled output) lists a
# SHA-256 for every file of the build tree (not target, node_modules, dist or .git
# directories) as of the last build that fully succeeded. `refresh` gives every file a
# current mtime if it is new or different from the manifest, so cargo recompiles exactly
# the crates owning those files whatever the original timestamps were. Files whose
# content did not change keep their mtimes, so unchanged crates stay cached; registry
# dependencies are not in the tree and are never touched. A deleted file is already
# detected by cargo. With no manifest (first build, or a cleared cache) every file is
# touched.
#
# A failed build is the hard case. Cache-mount writes survive it but the touched mtimes
# do not, and the compiled output may be partly built from the failed attempt's content.
# So `refresh` also records every path it touches in <manifest>.pending, and touches
# those paths again on every later run until a build succeeds, even if the content has
# since been reverted to the manifest's. It writes the current hashes to <manifest>.new
# and does not install them. After every compile has succeeded the build runs `commit`,
# which moves <manifest>.new into place and clears the pending list.
#
# usage: refresh-changed-mtimes.sh refresh <tree> <manifest>
#        refresh-changed-mtimes.sh commit <manifest>
set -eu
mode="${1:?mode: refresh or commit}"

if [ "$mode" = commit ]; then
  manifest="${2:?manifest}"
  mv "$manifest.new" "$manifest"
  rm -f "$manifest.pending"
  exit 0
fi
[ "$mode" = refresh ] || { echo "refresh-changed-mtimes: unknown mode $mode" >&2; exit 2; }

tree="${2:?tree}"
manifest="${3:?manifest}"
cd "$tree"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
# Each step runs on its own so a failing find or sha256sum stops the script (set -e).
find . \( -path ./target -o \( -type d \( -name node_modules -o -name dist -o -name .git \) \) \) -prune \
  -o -type f -print0 > "$tmp/list"
xargs -0 sha256sum < "$tmp/list" > "$tmp/sums"
# A sorted list of "<sha256>  <path>" lines, so comm can diff it against the manifest.
LC_ALL=C sort "$tmp/sums" > "$tmp/new"
if [ -f "$manifest" ]; then
  LC_ALL=C comm -13 "$manifest" "$tmp/new" > "$tmp/changed"
else
  cp "$tmp/new" "$tmp/changed"
fi
# Strip "<sha256>  " (64 hex digits and two spaces); add the paths still pending.
sed 's/^[0-9a-f]\{64\}  //' "$tmp/changed" > "$tmp/paths"
if [ -f "$manifest.pending" ]; then cat "$manifest.pending" >> "$tmp/paths"; fi
# Drop paths that no longer exist, de-duplicate, touch.
: > "$tmp/dirty"
LC_ALL=C sort -u "$tmp/paths" | while IFS= read -r p; do
  if [ -e "$p" ]; then printf '%s\n' "$p"; fi
done > "$tmp/dirty"
tr '\n' '\0' < "$tmp/dirty" | xargs -0 -r touch --
mkdir -p "$(dirname "$manifest")"
cp "$tmp/dirty" "$manifest.pending"
cp "$tmp/new" "$manifest.new"
echo "refresh-changed-mtimes: $(wc -l < "$tmp/dirty" | tr -d ' ') file(s) new, changed or pending since the last successful build; mtimes refreshed"
