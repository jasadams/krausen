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
export KRAUSEN_MODE=self_hosted PORT=41465
export BASE_URL="http://localhost:$PORT" FRONTEND_URL="http://localhost:$PORT"
export JWT_SECRET_KEY='test-only-jwt-secret-for-disposable-browser-fixture'
export ENCRYPTION_KEY='AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='
printf '%s\n' 'TestPassword1234' | target/debug/krausen bootstrap-owner local@localhost
./target/debug/krausen > "$fixture/server.log" 2>&1 &
server_pid=$!
attempt=0
until curl --silent --fail "$BASE_URL/login" > /dev/null; do
  attempt=$((attempt+1))
  if [ "$attempt" -ge 60 ]; then cat "$fixture/server.log"; exit 1; fi
  sleep 1
done
cd e2e
./node_modules/.bin/playwright test tests/recipes/editor.spec.ts -g 'equipment profiles can be saved|an uncertain equipment save|showcase kit supports|shared kit native popovers' --reporter=line
editor_state_pattern='ordinary bookmark|cleared print task|initial recovery authorization|obsolete preview context|explicit save conflict follow-up|a late committed save|typed print denial|successful revalidation preserves|revalidation never steals|a disposed editor|recovered older-base conflict|raw formatting settles|all numeric caller families|local retention failure|preview details and calculating|native input undo|context network failure|calculation retry uses|initial recipe load failure|authorized focus revalidation|a delayed save freezes|an explicit save keeps|shared preview exposes|editor controls remain usable|editor state controls|a dirty new recipe|undo and retained raw draft|navigation offers stay|stale saves retain edits|browser back keeps|retained draft stays isolated|save and leave waits|revoked credentials|recovered uncertain create|separate local drafts|a pending save|clean focus refetch'
./node_modules/.bin/playwright test tests/recipes/editor.spec.ts -g "$editor_state_pattern" --workers=1 --reporter=line --list
./node_modules/.bin/playwright test tests/recipes/editor.spec.ts -g "$editor_state_pattern" --workers=1 --reporter=line
./node_modules/.bin/playwright test tests/recipes/beerxml.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/recipes/library.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/recipes/print.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/recipes/catalogue.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/recipes/scaling.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/recipes/brew-start.spec.ts --reporter=line
# The populated history assertion in brew-start.spec.ts is env-gated by
# KRAUSEN_VISUAL_BREW_ID and is exercised manually against the exact visual
# fixture runtime; it is intentionally skipped in this default CI flow.
./node_modules/.bin/playwright test tests/recipes/design-fidelity.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/library/shell.spec.ts --reporter=line
./node_modules/.bin/playwright test tests/recipes/editor.spec.ts -g 'profile pagination exposes' --reporter=line
