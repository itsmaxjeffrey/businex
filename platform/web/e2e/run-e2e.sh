#!/usr/bin/env bash
set -euo pipefail

# Local E2E: disposable database, local API, built web app, Playwright.
# Proxy env is stripped from every child; probes use curl --noproxy.

unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy NODE_USE_ENV_PROXY || true
export NO_PROXY=127.0.0.1,localhost
export no_proxy=127.0.0.1,localhost

REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
WEB=$REPO/platform/web
ENVF="${BUSINEX_DEV_TEST_ENV:-/home/coffee/.local/state/businex/dev-test.env}"

set -a
. "$ENVF"
set +a

E2E_DB="businex_e2e_$(date +%s)"
ADMIN_BASE="${BUSINEX_TEST_DATABASE_URL%/*}"
ADMIN_URL="$ADMIN_BASE/$E2E_DB"
APP_URL="postgres://businex_app:businex-app-test-pw@127.0.0.1:55432/$E2E_DB"

echo "e2e: database $E2E_DB"
docker exec businex-dev-postgres-1 psql -U businex -d postgres -q -c "CREATE DATABASE \"$E2E_DB\""
docker exec businex-dev-postgres-1 psql -U businex -d postgres -q -c "ALTER ROLE businex_app LOGIN PASSWORD 'businex-app-test-pw'"
docker exec businex-dev-postgres-1 psql -U businex -d postgres -q -c "ALTER ROLE businex_service LOGIN PASSWORD 'businex-service-test-pw'"

API_PID=""
WEB_PID=""
cleanup() {
  [ -n "$API_PID" ] && kill "$API_PID" 2>/dev/null || true
  [ -n "$WEB_PID" ] && kill "$WEB_PID" 2>/dev/null || true
  sleep 0.3
  docker exec businex-dev-postgres-1 psql -U businex -d postgres -q -c "DROP DATABASE IF EXISTS \"$E2E_DB\"" || true
}
trap cleanup EXIT

# Build the current tree first: the harness must never silently exercise a
# stale target/debug binary.
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --manifest-path "$REPO/platform/Cargo.toml" -p businex-api
echo "e2e: api built from current sources"

BUSINEX_HOST=127.0.0.1 BUSINEX_PORT=8790 \
BUSINEX_DATABASE_URL="$APP_URL" \
BUSINEX_DATABASE_ADMIN_URL="$ADMIN_URL" \
BUSINEX_DEV_LOGIN=true BUSINEX_COOKIE_SECURE=false \
BUSINEX_REDIS_URL="$BUSINEX_TEST_REDIS_URL" \
  "$REPO/platform/target/debug/businex-api" > /tmp/businex-e2e-api.log 2>&1 &
API_PID=$!

api_ok=false
for i in $(seq 1 60); do
  if curl -s --noproxy '*' -f http://127.0.0.1:8790/healthz >/dev/null 2>&1; then api_ok=true; break; fi
  sleep 0.25
done
if [ "$api_ok" != true ]; then echo "e2e: api failed"; tail -25 /tmp/businex-e2e-api.log; exit 1; fi
echo "e2e: api healthy"

cd "$WEB"
npm run build > /tmp/businex-e2e-build.log 2>&1 || { echo "e2e: build failed"; tail -30 /tmp/businex-e2e-build.log; exit 1; }
echo "e2e: build ok"

BUSINEX_API_ORIGIN=http://127.0.0.1:8790 npm run preview -w @businex/desktop > /tmp/businex-e2e-web.log 2>&1 &
WEB_PID=$!

web_ok=false
for i in $(seq 1 60); do
  if curl -s --noproxy '*' -f http://127.0.0.1:4174/ >/dev/null 2>&1; then web_ok=true; break; fi
  sleep 0.25
done
if [ "$web_ok" != true ]; then echo "e2e: web failed"; tail -25 /tmp/businex-e2e-web.log; exit 1; fi
echo "e2e: web healthy"

cd "$WEB"
BUSINEX_WEB_ORIGIN=http://127.0.0.1:4174 npx playwright test 2>&1 | tee /tmp/businex-e2e-run.log
exit ${PIPESTATUS[0]}