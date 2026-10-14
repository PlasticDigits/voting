#!/usr/bin/env python3
"""Read-only release checks. No signatures, writes, credentials or load bursts."""
import argparse
import datetime
import json
import sys
import urllib.error
import urllib.request


def request(url, method="GET", headers=None):
    req = urllib.request.Request(url, method=method, headers=headers or {})
    try:
        response = urllib.request.urlopen(req, timeout=20)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.code, response.headers, response.read(2 * 1024 * 1024)


def run(api, frontend):
    checks = []

    def record(name, passed, detail=None):
        checks.append({"check": name, "passed": bool(passed), "detail": detail})

    code, headers, body = request(api + "/health")
    health = json.loads(body)
    record("API liveness", code == 200 and health.get("ok") is True, code)
    code, headers, body = request(api + "/ready")
    readiness = json.loads(body)
    record("dual-chain readiness", code == 200 and readiness.get("ready") is True, code)
    code, headers, body = request(api + "/v1/metadata")
    metadata = json.loads(body)
    record("configured protocol metadata", code == 200 and metadata.get("protocol_version") == 2
           and str(metadata.get("min_proposal_raw", "")).isdigit()
           and {"terra", "bsc"}.issubset(metadata.get("capabilities", [])), code)
    code, headers, body = request(api + "/openapi.json")
    spec = json.loads(body)
    paths = spec.get("paths", {})
    record("generated OpenAPI", code == 200 and spec.get("components", {}).get("schemas")
           and "put" in paths.get("/v1/proposals/{id}/sections", {}), len(paths))
    code, headers, body = request(api + "/v1/proposals")
    record("proposal read", code == 200 and isinstance(json.loads(body), list), code)
    target = api + "/v1/proposals/00000000-0000-0000-0000-000000000000/sections"
    preflight = {"Origin": frontend, "Access-Control-Request-Method": "PUT",
                 "Access-Control-Request-Headers": "content-type"}
    code, headers, body = request(target, "OPTIONS", preflight)
    record("allowed PUT preflight", code in (200, 204)
           and headers.get("Access-Control-Allow-Origin") == frontend
           and "PUT" in headers.get("Access-Control-Allow-Methods", ""), code)
    preflight["Origin"] = "https://untrusted.example"
    code, headers, body = request(target, "OPTIONS", preflight)
    record("untrusted origin rejected", headers.get("Access-Control-Allow-Origin") is None, code)
    code, headers, body = request(api + "/v1/balances/invalid-wallet?chain=bsc")
    error = json.loads(body)
    record("stable public error", 400 <= code < 500 and isinstance(error.get("code"), str)
           and isinstance(error.get("request_id"), str)
           and headers.get("X-Request-ID") == error.get("request_id")
           and "postgres" not in body.decode().lower(), code)
    for path in ("/", "/new", "/vote", "/vote/new"):
        code, headers, body = request(frontend + path)
        record("SPA " + path, code == 200 and "text/html" in headers.get("Content-Type", "")
               and headers.get("X-Frame-Options") == "DENY"
               and bool(headers.get("Content-Security-Policy")), code)
    code, headers, body = request(frontend + "/assets/missing-release-probe.js")
    record("missing asset is 404", code == 404, code)
    return {"observed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "api": api, "frontend": frontend, "checks": checks,
            "passed": all(check["passed"] for check in checks)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api", default="https://operator.vote.cl8y.com")
    parser.add_argument("--frontend", default="https://vote.cl8y.com")
    args = parser.parse_args()
    try:
        result = run(args.api.rstrip("/"), args.frontend.rstrip("/"))
        print(json.dumps(result, indent=2))
        sys.exit(0 if result["passed"] else 1)
    except Exception as error:
        # Never print arbitrary transport URLs, response bodies or credentials.
        print(json.dumps({"passed": False, "error_class": type(error).__name__}))
        sys.exit(1)
