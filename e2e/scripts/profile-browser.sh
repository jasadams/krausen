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
./node_modules/.bin/playwright test tests/recipes/editor.spec.ts -g 'equipment profiles can be saved|an uncertain equipment save|profile pagination exposes' --reporter=line
./node_modules/.bin/playwright test tests/recipes/beerxml.spec.ts --reporter=line
