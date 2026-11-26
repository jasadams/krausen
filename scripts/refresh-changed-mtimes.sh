#!/bin/sh
# Make cargo's mtime-based freshness agree with the content of the build context.
#
# Cargo calls a workspace crate fresh when none of its source files is newer than the
# crate's cached dep-info file. `COPY . .` keeps the source files' original mtimes, so a
# file whose mtime is older than the cached build output (an edit made while an earlier
# build ran, a `git checkout` of older files, `touch -t`) looks unchanged and the image
# keeps the old code. This script records a SHA-256 of every file in the build tree (not target, node_modules, dist or .git directories) next
# to the compiled output (inside the target cache mount) and, before cargo runs, gives
# every file whose content is new or different since the last recorded build a current
# mtime. Cargo then recompiles exactly the crates owning those files, whatever the
# original timestamps were. Files whose content did not change keep their mtimes, so
# unchanged crates stay cached; registry dependencies are not in the tree and are
# never touched. A deleted file is already detected by cargo (its dep-info entry is
# missing). With no manifest (first build, or a cleared cache) every file is touched.
#
# usage: refresh-changed-mtimes.sh <tree> <manifest>
set -eu
tree="${1:?tree}"
manifest="${2:?manifest}"
cd "$tree"
new="$(mktemp)"
trap 'rm -f "$new" "$new.list" "$new.sums" "$new.paths"' EXIT
# The manifest must be a sorted list of "<sha256>  <path>" lines so comm can diff it.
# Each step runs on its own so a failing find or sha256sum stops the script (set -e).
find . \( -path ./target -o \( -type d \( -name node_modules -o -name dist -o -name .git \) \) \) -prune \
  -o -type f -print0 > "$new.list"
xargs -0 sha256sum < "$new.list" > "$new.sums"
LC_ALL=C sort "$new.sums" > "$new"
if [ -f "$manifest" ]; then
  LC_ALL=C comm -13 "$manifest" "$new" > "$new.paths"
else
  cp "$new" "$new.paths"
fi
# Strip "<sha256>  " (64 hex digits and two spaces) and touch each path.
sed 's/^[0-9a-f]\{64\}  //' "$new.paths" | tr '\n' '\0' | xargs -0 -r touch --
count="$(wc -l < "$new.paths" | tr -d ' ')"
mkdir -p "$(dirname "$manifest")"
cp "$new" "$manifest.new"
echo "refresh-changed-mtimes: $count file(s) new or changed since the last build; mtimes refreshed"
