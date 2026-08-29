#!/bin/sh
# Smoke-test SPA fallback for issue #8 / invariant O8.
#
# GET /, /vote, /new, /vote/new, /vote/:id, /:id → 200 HTML (index.html).
# GET /assets/<missing>.js → 404 (static try_files $uri =404 must stay).
#
# Local (non-root): wraps nginx:1.27-alpine via Docker.
# CI: GitLab job uses the nginx image as root and runs this in-process.
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
NGINX_CONF="$ROOT/deploy/docker/frontend.nginx.conf"
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
  HEADERS="$ROOT/deploy/docker/frontend.security-headers.conf"
fi

cp "$NGINX_CONF" /etc/nginx/conf.d/default.conf
mkdir -p /etc/nginx/snippets /usr/share/nginx/html/assets
cp "$HEADERS" /etc/nginx/snippets/voting-security-headers.conf
printf '%s\n' '<!doctype html><html><head><title>voting spa</title></head><body>spa-index</body></html>' \
  > /usr/share/nginx/html/index.html
printf '%s\n' 'console.log(1)' > /usr/share/nginx/html/assets/app.js
printf '%s\n' 'not-the-spa' > /usr/share/nginx/html/favicon.svg

nginx -t
nginx -g 'daemon on;'

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

assert_200_html /
assert_200_html /vote
assert_200_html /vote/
assert_200_html /vote/new
assert_200_html /new
assert_200_html /vote/does-not-exist-uuid
assert_200_html /does-not-exist-uuid

assert_404 /assets/missing-file.js
# Real hashed file is served as the asset, not the SPA document.
body="$(mktemp)"
wget -qS -O "$body" "http://127.0.0.1/assets/app.js" 2>/dev/null
grep -q "console.log" "$body"
rm -f "$body"

# Root extension files must not SPA-fallback (regex location).
assert_404 /missing-icon.ico

nginx -s stop
echo "SPA fallback smoke OK"
