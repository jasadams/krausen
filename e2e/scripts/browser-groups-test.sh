#!/bin/sh
# Self-test for the path-to-group selection in browser-groups.mjs.
# Run: sh e2e/scripts/browser-groups-test.sh
set -u
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
fail=0
expect() { # name want(space separated, sorted) files...
  name=$1; want=$2; shift 2
  : > "$tmp/files"
  for f in "$@"; do printf '%s\n' "$f" >> "$tmp/files"; done
  got=$(node "$here/browser-groups.mjs" select --files "$tmp/files" | sort | tr '\n' ' ' | sed 's/ $//')
  if [ "$got" = "$want" ]; then echo "PASS $name"; else echo "FAIL $name: got '$got', wanted '$want'"; fail=1; fi
}
expect "empty diff runs smoke" "library wayfinding"
expect "docs only runs nothing" "" docs/UX_DESIGN.md README.md
expect "an unmapped path runs smoke" "library wayfinding" crates/krausen-ui/src/pages/welcome.rs
expect "a mapped path runs its group" "brew-start" crates/krausen-ui/src/pages/brew_start.rs
expect "a mapped path plus docs adds nothing" "brew-start" crates/krausen-ui/src/pages/brew_start.rs docs/UX_DESIGN.md
all=$(node -e "console.log(Object.keys(JSON.parse(require('fs').readFileSync('$here/../browser-groups.json')).groups).sort().join(' '))")
for f in crates/krausen-ui/src/lib.rs crates/krausen-ui/src/main.rs crates/krausen-ui/src/pages/mod.rs crates/krausen-ui/src/types.rs crates/krausen-app/src/lib.rs crates/krausen-ui/src/app.rs e2e/helpers/test-helpers.ts; do
  expect "cross-cutting $f runs every group" "$all" "$f"
done
[ "$fail" -eq 0 ] && echo "all checks passed"
exit $fail
