#!/bin/sh
# Build the server image and prove it runs (backlog 046, gap G12).
#
# Usage: deploy/verify-image.sh
#   Run from anywhere; needs docker and curl. The CI job `image` runs it.
#
# What it checks:
#   1. The customer image (no cargo features) builds, runs as a non-root
#      user, and refuses to start without a license. That last check proves
#      the `dev-license` feature is not in it (docs/release-checklist.md).
#   2. A test image (`CARGO_FEATURES=dev-license`, as on Railway) starts with
#      password login, answers `GET /healthz` with 200, and answers
#      `GetLoginOptions` over gRPC-Web with the server name and status 0.
#
# The images are tagged `aegistara-server:verify-customer` and
# `aegistara-server:verify-test`, and the container is removed at the end,
# also after a failure. Cost: two release builds of the server.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_root"

for tool in docker curl; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: $tool is not installed." >&2
        exit 127
    fi
done

customer=aegistara-server:verify-customer
tested=aegistara-server:verify-test
name="aegis-verify-$$"
port=${VERIFY_PORT:-18051}

cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

fail() {
    echo "FAIL  $*" >&2
    docker logs "$name" >&2 2>&1 || true
    exit 1
}

echo "== build the customer image"
docker build -f deploy/Dockerfile -t "$customer" .

user=$(docker inspect --format '{{.Config.User}}' "$customer")
[ "$user" = "nonroot" ] || fail "the image runs as '$user', not nonroot"
echo "ok    runs as nonroot"

# No license file and no dev-license feature: the server must stop at once
# and say why. `|| true` because a non-zero exit is the expected result.
out=$(docker run --rm --name "$name" \
    -e AEGIS_AUTH=password -e AEGIS_ADMIN_PASSWORD='verify image password' \
    -e AEGIS_DEV_LICENSE=1 "$customer" 2>&1) && status=0 || status=$?
[ "$status" -ne 0 ] || fail "the customer image started without a license"
echo "$out" | grep -q "AEGIS_LICENSE_FILE is required" ||
    fail "the customer image stopped for another reason: $out"
echo "ok    the customer image refuses to start without a license"

for dev_env in "AEGIS_AUTH=dev" "AEGIS_AUTH=development" "AEGIS_DEV_AUTH=1"; do
    out=$(docker run --rm --name "$name" -e "$dev_env" "$customer" 2>&1) && status=0 || status=$?
    [ "$status" -ne 0 ] || fail "the customer image accepted $dev_env"
    echo "$out" | grep -q "not enabled in this build" ||
        fail "the customer image stopped for an unexpected reason on $dev_env: $out"
done
echo "ok    the customer image refuses all development-auth aliases"

echo "== build the test image"
docker build -f deploy/Dockerfile --build-arg CARGO_FEATURES=dev-license -t "$tested" .

docker run -d --name "$name" -p "127.0.0.1:$port:50051" \
    -e AEGIS_AUTH=password -e AEGIS_ADMIN_PASSWORD='verify image password' \
    -e AEGIS_SERVER_NAME='Image check' -e AEGIS_DEV_LICENSE=1 "$tested" >/dev/null

tries=0
until [ "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/healthz" || true)" = 200 ]; do
    tries=$((tries + 1))
    [ "$tries" -lt 30 ] || fail "/healthz did not answer 200 within 30 seconds"
    sleep 1
done
echo "ok    /healthz answers 200"

# An empty gRPC-Web request: one frame, flag 0, length 0.
reply=$(printf '\0\0\0\0\0' | curl -s --max-time 10 \
    -H 'content-type: application/grpc-web+proto' -H 'x-grpc-web: 1' \
    --data-binary @- "http://127.0.0.1:$port/aegis.v1.Accounts/GetLoginOptions" | tr -d '\0' | strings)
echo "$reply" | grep -q "Image check" || fail "GetLoginOptions did not return the server name: $reply"
echo "$reply" | grep -qi "grpc-status: *0" || fail "GetLoginOptions did not end with status 0: $reply"
echo "ok    GetLoginOptions over gRPC-Web names the server"

echo "all image checks passed"
