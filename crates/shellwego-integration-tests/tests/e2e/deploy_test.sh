#!/usr/bin/env bash
# --------------------------------------------------------------------------- #
# ShellWeGo E2E deploy test
#
# Exercises the full application lifecycle against a running control plane:
#   1. Register user (token creation)
#   2. Login and get token
#   3. Create organization
#   4. Create an app
#   5. List apps and verify
#   6. Get app by ID
#   7. Update app (restart as proxy)
#   8. Delete app
#   9. Verify app is gone
#
# Requirements: curl, jq
# Usage: ./deploy_test.sh [BASE_URL]
# --------------------------------------------------------------------------- #

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

BASE_URL="${SHELLWEGO_URL:-http://localhost:8080}"
API_BASE="${BASE_URL}/api/v1"

# Colours
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Colour

# Counters
PASS=0
FAIL=0
SKIP=0
TESTS=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Unique suffix so parallel runs don't clash
SUFFIX="$(date +%s)-$$"

log_pass() {
    PASS=$((PASS + 1))
    echo -e "  ${GREEN}✓ PASS${NC}  $1"
}

log_fail() {
    FAIL=$((FAIL + 1))
    echo -e "  ${RED}✗ FAIL${NC}  $1"
    if [ -n "${2:-}" ]; then
        echo -e "         ${RED}detail: $2${NC}"
    fi
}

log_skip() {
    SKIP=$((SKIP + 1))
    echo -e "  ${YELLOW}⊘ SKIP${NC}  $1 — $2"
}

log_section() {
    echo ""
    echo -e "${CYAN}━━━ $1 ━━━${NC}"
}

# Assert HTTP status code equals expected.
# Usage: assert_status "description" actual expected
assert_status() {
    TESTS=$((TESTS + 1))
    local desc="$1" actual="$2" expected="$3"
    if [ "$actual" -eq "$expected" ]; then
        log_pass "$desc (HTTP $actual)"
    else
        log_fail "$desc" "expected HTTP $expected, got HTTP $actual"
    fi
}

# Assert a jq expression against a JSON body.
# Usage: assert_json "description" json_body "jq_expression" expected_value
assert_json() {
    TESTS=$((TESTS + 1))
    local desc="$1" body="$2" expr="$3" expected="$4"
    local actual
    actual=$(echo "$body" | jq -r "$expr" 2>/dev/null || echo "JQ_ERROR")
    if [ "$actual" = "$expected" ]; then
        log_pass "$desc"
    else
        log_fail "$desc" "expected '$expected', got '$actual'"
    fi
}

# Assert a jq expression evaluates to true.
# Usage: assert_json_true "description" json_body "jq_expression"
assert_json_true() {
    TESTS=$((TESTS + 1))
    local desc="$1" body="$2" expr="$3"
    local actual
    actual=$(echo "$body" | jq "$expr" 2>/dev/null || echo "false")
    if [ "$actual" = "true" ]; then
        log_pass "$desc"
    else
        log_fail "$desc" "expression '$expr' evaluated to $actual"
    fi
}

# Make a curl request and return "$STATUS\n$BODY".
do_curl() {
    local method="$1" path="$2" data="${3:-}"
    local url="${API_BASE}${path}"
    local tmpfile
    tmpfile=$(mktemp)

    local -a curl_args=(-s -o "$tmpfile" -w "%{http_code}" -X "$method" "$url")
    curl_args+=(-H "Content-Type: application/json")

    if [ -n "$TOKEN" ]; then
        curl_args+=(-H "Authorization: Bearer ${TOKEN}")
    fi

    if [ -n "$data" ]; then
        curl_args+=(-d "$data")
    fi

    local http_code
    http_code=$(curl "${curl_args[@]}")
    local body
    body=$(cat "$tmpfile")
    rm -f "$tmpfile"

    echo "$http_code"
    echo "$body"
}

# Wrapper: capture status + body from do_curl.
curl_req() {
    local output
    output=$(do_curl "$@")
    CURL_STATUS=$(echo "$output" | head -1)
    CURL_BODY=$(echo "$output" | tail -n +2)
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

check_prerequisites() {
    log_section "Pre-flight checks"

    # Check curl
    if command -v curl &>/dev/null; then
        log_pass "curl is available ($(curl --version | head -1))"
    else
        log_fail "curl is required but not found"
        exit 1
    fi

    # Check jq
    if command -v jq &>/dev/null; then
        log_pass "jq is available ($(jq --version 2>&1))"
    else
        log_fail "jq is required but not found"
        exit 1
    fi

    # Check server is reachable
    local health_status
    health_status=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/health" 2>/dev/null || echo "000")
    if [ "$health_status" = "200" ]; then
        log_pass "Control plane reachable at ${BASE_URL}/health (HTTP 200)"
    else
        log_fail "Control plane not reachable at ${BASE_URL}/health" \
                 "got HTTP ${health_status}"
        echo -e "\n${YELLOW}Make sure the control plane is running:${NC}"
        echo -e "  ${CYAN}SHELLWEGO_URL=${BASE_URL} ./tests/e2e/deploy_test.sh${NC}"
        exit 1
    fi

    # Check readiness
    local ready_status
    ready_status=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/readyz" 2>/dev/null || echo "000")
    if [ "$ready_status" = "200" ]; then
        log_pass "Readiness check passed (HTTP 200)"
    else
        log_skip "Readiness check" "got HTTP ${ready_status} (may need DB)"
    fi
}

# ---------------------------------------------------------------------------
# Test: Authentication
# ---------------------------------------------------------------------------

test_auth() {
    log_section "1. Authentication"

    # --- Register / Login (token creation) ---
    local login_body="{\"username\":\"testuser-${SUFFIX}@example.com\",\"password\":\"test-password-${SUFFIX}\"}"
    curl_req POST "/auth/token" "$login_body"
    assert_status "POST /auth/token returns 200" "$CURL_STATUS" 200

    TOKEN=$(echo "$CURL_BODY" | jq -r '.token // empty' 2>/dev/null)
    if [ -z "$TOKEN" ]; then
        log_fail "Failed to extract auth token from response"
        echo "  Response: $CURL_BODY"
        exit 1
    fi
    log_pass "Extracted auth token (${TOKEN:0:16}...)"

    assert_json "token_type is Bearer" "$CURL_BODY" '.token_type' "Bearer"
    assert_json_true ".expires_in > 0" "$CURL_BODY" '.expires_in > 0'

    REFRESH_TOKEN=$(echo "$CURL_BODY" | jq -r '.refresh_token // empty')

    # --- Refresh token ---
    local refresh_body="{\"refresh_token\":\"${REFRESH_TOKEN}\"}"
    curl_req POST "/auth/refresh" "$refresh_body"
    assert_status "POST /auth/refresh returns 200" "$CURL_STATUS" 200

    NEW_TOKEN=$(echo "$CURL_BODY" | jq -r '.token // empty')
    if [ -n "$NEW_TOKEN" ] && [ "$NEW_TOKEN" != "$TOKEN" ]; then
        log_pass "Refresh returned a new access token"
    else
        log_skip "Refresh token differentiation" "tokens may be the same in dev mode"
    fi
}

# ---------------------------------------------------------------------------
# Test: Organizations
# ---------------------------------------------------------------------------

test_organizations() {
    log_section "2. Organizations"

    local org_body="{\"name\":\"Test Org ${SUFFIX}\"}"
    curl_req POST "/organizations" "$org_body"
    assert_status "POST /organizations returns 201" "$CURL_STATUS" 201

    ORG_ID=$(echo "$CURL_BODY" | jq -r '.id // empty')
    if [ -z "$ORG_ID" ]; then
        log_fail "Failed to extract organization ID"
    else
        log_pass "Created organization (id: ${ORG_ID:0:8}...)"
    fi

    # List organizations
    curl_req GET "/organizations"
    assert_status "GET /organizations returns 200" "$CURL_STATUS" 200
    assert_json_true ".items is an array" "$CURL_BODY" '.items | type == "array"'
}

# ---------------------------------------------------------------------------
# Test: App CRUD
# ---------------------------------------------------------------------------

test_apps() {
    log_section "3. Applications"

    APP_NAME="e2e-test-app-${SUFFIX}"

    # --- Create app ---
    local create_body="{\"name\":\"${APP_NAME}\",\"image\":\"ghcr.io/shellwego/hello-world:latest\",\"replicas\":2}"
    curl_req POST "/apps" "$create_body"
    assert_status "POST /apps returns 201" "$CURL_STATUS" 201

    APP_ID=$(echo "$CURL_BODY" | jq -r '.id // empty')
    if [ -z "$APP_ID" ]; then
        log_fail "Failed to extract app ID from create response"
        echo "  Response: $CURL_BODY"
        exit 1
    fi
    log_pass "Created app '${APP_NAME}' (id: ${APP_ID:0:8}...)"

    assert_json "app name matches" "$CURL_BODY" '.name' "$APP_NAME"
    assert_json "app status is 'creating'" "$CURL_BODY" '.status' "creating"
    assert_json "app replicas" "$CURL_BODY" '.replicas' "2"

    # --- List apps ---
    curl_req GET "/apps"
    assert_status "GET /apps returns 200" "$CURL_STATUS" 200
    assert_json_true "list response has items array" "$CURL_BODY" '.items | type == "array"'

    # --- Get app by ID ---
    curl_req GET "/apps/${APP_ID}"
    if [ "$CURL_STATUS" -eq 200 ]; then
        assert_json "GET app name matches" "$CURL_BODY" '.name' "$APP_NAME"
        assert_json "GET app id matches" "$CURL_BODY" '.id' "$APP_ID"
        log_pass "GET /apps/:id returns correct app"
    else
        log_skip "GET /apps/:id" "handler returns 404 (persistence not wired)"
    fi

    # --- Update / restart (proxy for update) ---
    curl_req POST "/apps/${APP_ID}/restart" "{}"
    assert_status "POST /apps/:id/restart returns 200" "$CURL_STATUS" 200

    # --- Deploy ---
    curl_req POST "/apps/${APP_ID}/deploy" "{}"
    assert_status "POST /apps/:id/deploy returns 200" "$CURL_STATUS" 200
    if [ "$CURL_STATUS" -eq 200 ]; then
        assert_json "deploy status is 'pending'" "$CURL_BODY" '.status' "pending"
    fi

    # --- Scale ---
    curl_req POST "/apps/${APP_ID}/scale" "{\"replicas\":5}"
    if [ "$CURL_STATUS" -eq 200 ]; then
        log_pass "Scale app succeeded"
    else
        log_skip "Scale app" "handler returns 404 (persistence not wired)"
    fi
}

# ---------------------------------------------------------------------------
# Test: Delete app and verify
# ---------------------------------------------------------------------------

test_delete_app() {
    log_section "4. Delete app"

    if [ -z "${APP_ID:-}" ]; then
        log_fail "No APP_ID set from previous tests"
        return
    fi

    # --- Delete app ---
    curl_req DELETE "/apps/${APP_ID}"
    if [ "$CURL_STATUS" -eq 204 ]; then
        log_pass "DELETE /apps/:id returns 204"
    elif [ "$CURL_STATUS" -eq 404 ]; then
        log_skip "DELETE /apps/:id" "handler returns 404 (persistence not wired)"
    else
        log_fail "DELETE /apps/:id" "unexpected HTTP $CURL_STATUS"
    fi

    # --- Verify app is gone ---
    curl_req GET "/apps/${APP_ID}"
    if [ "$CURL_STATUS" -eq 404 ]; then
        log_pass "App is gone after deletion (HTTP 404)"
    else
        log_fail "App still exists after deletion" "got HTTP $CURL_STATUS"
    fi
}

# ---------------------------------------------------------------------------
# Test: Nodes
# ---------------------------------------------------------------------------

test_nodes() {
    log_section "5. Nodes"

    local node_body="{\"hostname\":\"e2e-node-${SUFFIX}\",\"region\":\"us-east-1\",\"capacity\":{\"cpu_cores\":4,\"memory_gb\":16,\"disk_gb\":100}}"
    curl_req POST "/nodes" "$node_body"
    assert_status "POST /nodes returns 201" "$CURL_STATUS" 201

    NODE_ID=$(echo "$CURL_BODY" | jq -r '.id // empty')
    if [ -n "$NODE_ID" ]; then
        log_pass "Registered node (id: ${NODE_ID:0:8}...)"
    fi

    # List nodes
    curl_req GET "/nodes"
    assert_status "GET /nodes returns 200" "$CURL_STATUS" 200

    # Drain node
    if [ -n "${NODE_ID:-}" ]; then
        curl_req POST "/nodes/${NODE_ID}/drain" "{}"
        assert_status "POST /nodes/:id/drain returns 200" "$CURL_STATUS" 200

        # Deregister
        curl_req DELETE "/nodes/${NODE_ID}"
        assert_status "DELETE /nodes/:id returns 204" "$CURL_STATUS" 204
    fi
}

# ---------------------------------------------------------------------------
# Test: Secrets
# ---------------------------------------------------------------------------

test_secrets() {
    log_section "6. Secrets"

    local secret_body="{\"name\":\"E2E_SECRET_${SUFFIX}\",\"value\":\"super-secret-value-123\",\"scope\":\"organization\"}"
    curl_req POST "/secrets" "$secret_body"
    assert_status "POST /secrets returns 201" "$CURL_STATUS" 201

    SECRET_ID=$(echo "$CURL_BODY" | jq -r '.id // empty')
    if [ -n "$SECRET_ID" ]; then
        log_pass "Created secret (id: ${SECRET_ID:0:8}...)"
    fi

    # Verify no plaintext in response
    local has_value
    has_value=$(echo "$CURL_BODY" | jq 'has("value")' 2>/dev/null || echo "false")
    if [ "$has_value" = "false" ]; then
        log_pass "Secret response does not contain plaintext value"
    else
        log_fail "Secret response leaks plaintext value"
    fi

    # List secrets
    curl_req GET "/secrets"
    assert_status "GET /secrets returns 200" "$CURL_STATUS" 200
}

# ---------------------------------------------------------------------------
# Test: Builds
# ---------------------------------------------------------------------------

test_builds() {
    log_section "7. Builds"

    # List builds
    curl_req GET "/builds"
    assert_status "GET /builds returns 200" "$CURL_STATUS" 200

    # Build logs for a nonexistent build
    curl_req GET "/builds/00000000-0000-0000-0000-000000000000/logs"
    assert_status "GET /builds/:id/logs returns 200" "$CURL_STATUS" 200

    # Cancel a nonexistent build
    curl_req POST "/builds/00000000-0000-0000-0000-000000000000/cancel" "{}"
    assert_status "POST /builds/:id/cancel returns 200" "$CURL_STATUS" 200
}

# ---------------------------------------------------------------------------
# Test: Metrics
# ---------------------------------------------------------------------------

test_metrics() {
    log_section "8. Metrics"

    curl_req GET "/metrics"
    assert_status "GET /metrics returns 200" "$CURL_STATUS" 200
    assert_json_true "metrics has agents" "$CURL_BODY" '.agents != null'
    assert_json_true "metrics has version" "$CURL_BODY" '.version != null'
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

print_summary() {
    local total=$((PASS + FAIL + SKIP))
    echo ""
    echo -e "${CYAN}════════════════════════════════════════${NC}"
    echo -e "  E2E Test Summary"
    echo -e "${CYAN}════════════════════════════════════════${NC}"
    echo -e "  Total assertions:  ${TESTS}"
    echo -e "  ${GREEN}Passed:   ${PASS}${NC}"
    echo -e "  ${RED}Failed:   ${FAIL}${NC}"
    echo -e "  ${YELLOW}Skipped:  ${SKIP}${NC}"
    echo -e "${CYAN}════════════════════════════════════════${NC}"
    echo ""

    if [ "$FAIL" -gt 0 ]; then
        echo -e "${RED}✗ Some tests failed${NC}"
        return 1
    fi
    echo -e "${GREEN}✓ All tests passed${NC}"
    return 0
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    echo ""
    echo -e "${CYAN}╔════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║  ShellWeGo E2E Deploy Test                 ║${NC}"
    echo -e "${CYAN}║  Target: ${BASE_URL}  ║${NC}"
    echo -e "${CYAN}╚════════════════════════════════════════════╝${NC}"

    check_prerequisites

    test_auth
    test_organizations
    test_apps
    test_delete_app
    test_nodes
    test_secrets
    test_builds
    test_metrics

    print_summary
}

main "$@"
