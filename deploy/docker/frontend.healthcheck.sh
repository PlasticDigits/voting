#!/bin/sh
# Image HEALTHCHECK for issue #8 / O8: SPA documents must be 200, not 404.
# Probes canonical and /vote alias paths. Missing assets are checked in CI
# (deploy/docker/test-frontend-spa-fallback.sh), not here.
set -eu
for path in / /vote /new /vote/new; do
  wget -qO- "http://127.0.0.1${path}" >/dev/null || exit 1
done
