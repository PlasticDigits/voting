#!/bin/sh
# Smoke-test SPA fallback for issue #8 / invariant O8, and O3 headers on both
# nginx configs Coolify may use:
#   - deploy/docker/frontend.nginx.conf (Dockerfile image, nginx 1.27)
#   - deploy/coolify-frontend.nginx.conf (pasteable Coolify static, often 1.31.x)
#
# GET /, /vote, /new, /vote/new, /vote/:id, /:id → 200 HTML (index.html).
# GET /assets/<missing>.js → 404 (static try_files $uri =404 must stay).
# SPA HTML must send X-Frame-Options DENY (O3). Missing assets must not be the SPA.
#
# Local (non-root): wraps nginx:1.27-alpine via Docker.
# CI: GitLab job uses the nginx image as root and runs this in-process.
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
NGINX_CONF="$ROOT/deploy/docker/frontend.nginx.conf"
COOLIFY_CONF="$ROOT/deploy/coolify-frontend.nginx.conf"
HEADERS="$ROOT/deploy/docker/frontend.security-headers.conf"

if [ "${SPA_FALLBACK_INNER:-}" != "1" ] && [ ! -w /etc/nginx/conf.d ] 2>/dev/null; then
  if command -v docker >/dev/null 2>&1; then
    exec docker run --rm \
      -e SPA_FALLBACK_INNER=1 \
      -v "$ROOT:/src:ro" \
      nginx:1.27-alpine \
      sh /src/deploy/docker/test-frontend-spa-fallback.sh
  fi
  echo "need a writable nginx conf.d (CI) or docker" >&2
  exit 1
fi

if [ "${SPA_FALLBACK_INNER:-}" = "1" ]; then
  ROOT=/src
  NGINX_CONF="$ROOT/deploy/docker/frontend.nginx.conf"
  COOLIFY_CONF="$ROOT/deploy/coolify-frontend.nginx.conf"
  HEADERS="$ROOT/deploy/docker/frontend.security-headers.conf"
fi

mkdir -p /etc/nginx/snippets /usr/share/nginx/html/assets
printf '%s\n' '<!doctype html><html><head><title>voting spa</title></head><body>spa-index</body></html>' \
  > /usr/share/nginx/html/index.html
printf '%s\n' 'console.log(1)' > /usr/share/nginx/html/assets/app.js
printf '%s\n' 'not-the-spa' > /usr/share/nginx/html/favicon.svg
cp "$HEADERS" /etc/nginx/snippets/voting-security-headers.conf

assert_200_html() {
  path="$1"
  body="$(mktemp)"
  hdr="$(mktemp)"
  if ! wget -qS -O "$body" "http://127.0.0.1${path}" 2>"$hdr"; then
    echo "FAIL $path: wget failed (expected 200 HTML)" >&2
    cat "$hdr" >&2
    exit 1
  fi
  if ! grep -q "HTTP/1.1 200" "$hdr"; then
    echo "FAIL $path: not HTTP 200" >&2
    cat "$hdr" >&2
    exit 1
  fi
  if ! grep -q "X-Frame-Options: DENY" "$hdr"; then
    echo "FAIL $path: missing X-Frame-Options DENY on SPA HTML" >&2
    cat "$hdr" >&2
    exit 1
  fi
  if ! grep -qi "spa-index" "$body"; then
    echo "FAIL $path: body is not SPA index.html" >&2
    cat "$body" >&2
    exit 1
  fi
  rm -f "$body" "$hdr"
}

assert_404() {
  path="$1"
  body="$(mktemp)"
  hdr="$(mktemp)"
  if wget -qS -O "$body" "http://127.0.0.1${path}" 2>"$hdr"; then
    echo "FAIL $path: expected 404, got success" >&2
    cat "$hdr" >&2
    exit 1
  fi
  if ! grep -q "HTTP/1.1 404" "$hdr"; then
    echo "FAIL $path: expected HTTP 404" >&2
    cat "$hdr" >&2
    exit 1
  fi
  if grep -qi "spa-index" "$body"; then
    echo "FAIL $path: 404 must not be SPA index.html" >&2
    exit 1
  fi
  rm -f "$body" "$hdr"
}

run_suite() {
  label="$1"
  conf="$2"
  echo "SPA fallback suite: $label"

  nginx -s stop 2>/dev/null || true
  # Alpine `nginx -s stop` returns before :80 is released; the next master
  # otherwise logs bind() EADDRINUSE and retries.
  i=0
  while [ "$i" -lt 20 ]; do
    if wget -qO- --timeout=1 "http://127.0.0.1/" >/dev/null 2>&1; then
      i=$((i + 1))
      sleep 0.1
      continue
    fi
    break
  done
  cp "$conf" /etc/nginx/conf.d/default.conf
  nginx -t
  nginx -g 'daemon on;'

  assert_200_html /
  assert_200_html /vote
  assert_200_html /vote/
  assert_200_html /vote/new
  assert_200_html /new
  assert_200_html /vote/does-not-exist-uuid
  assert_200_html /does-not-exist-uuid

  assert_404 /assets/missing-file.js
  body="$(mktemp)"
  wget -qS -O "$body" "http://127.0.0.1/assets/app.js" 2>/dev/null
  grep -q "console.log" "$body"
  rm -f "$body"

  # Root extension files must not SPA-fallback (regex location).
  assert_404 /missing-icon.ico

  nginx -s stop
  echo "SPA fallback smoke OK ($label)"
}

run_suite "frontend.nginx.conf (Dockerfile)" "$NGINX_CONF"
run_suite "coolify-frontend.nginx.conf (static paste)" "$COOLIFY_CONF"
echo "SPA fallback smoke OK (both nginx configs)"
