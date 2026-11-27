#!/bin/sh
# Run the profile browser scenario against an isolated disposable installation.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
fixture=$(mktemp -d)
cleanup() {
  if [ -n "${server_pid:-}" ]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$fixture"
}
trap cleanup EXIT INT TERM
cd "$root"
touch "$fixture/krausen.db"
export DATABASE_URL="sqlite://$fixture/krausen.db"
export KRAUSEN_E2E_DATABASE_PATH="$fixture/krausen.db"
. "$root/e2e/scripts/test-env.sh"
./target/debug/krausen > "$fixture/server.log" 2>&1 &
server_pid=$!
attempt=0
until curl --silent --fail "$BASE_URL/login" > /dev/null; do
  attempt=$((attempt+1))
  if [ "$attempt" -ge 60 ]; then cat "$fixture/server.log"; exit 1; fi
  sleep 1
done
cd e2e
# Which tests run is defined once, in e2e/browser-groups.json, and read by
# browser-groups.mjs. Usage: profile-browser.sh [GROUPS|all] [I/N]
# With no arguments it runs the whole selection, as before. A populated history
# assertion in brew-start.spec.ts is env-gated by KRAUSEN_VISUAL_BREW_ID and is
# exercised manually against the exact visual fixture runtime; it is
# intentionally skipped in this default flow.
node scripts/browser-groups.mjs run "${1:-all}" "${2:-1/1}"
