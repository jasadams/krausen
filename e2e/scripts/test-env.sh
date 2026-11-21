# Disposable test installation shared by e2e/scripts/profile-browser.sh and
# scripts/dev/krausen-dev: one definition of the port, mode, URLs and test-only
# secrets. Sourced, not executed. PORT may be set before sourcing.
KRAUSEN_MODE=self_hosted
PORT="${PORT:-41465}"
BASE_URL="http://localhost:$PORT"
FRONTEND_URL="http://localhost:$PORT"
JWT_SECRET_KEY='test-only-jwt-secret-for-disposable-browser-fixture'
ENCRYPTION_KEY='AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='
export KRAUSEN_MODE PORT BASE_URL FRONTEND_URL JWT_SECRET_KEY ENCRYPTION_KEY
