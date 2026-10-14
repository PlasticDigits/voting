#!/usr/bin/env python3
"""Deploy only the three inventoried voting services; credentials stay in memory.

Run under bao-exec --only COOLIFY_WIRE_TOKEN,COOLIFY_URL.
"""
import argparse
import json
import os
import re
import subprocess
import urllib.parse

RESOURCES = {
    "ledger": ("s6j6fx1870zbsmhbu2do1yiz", "voting-ledger"),
    "operator": ("x5j1g2xuc5p508g26ff003q0", "CL8Y Voting Operator"),
    "frontend": ("svvxk9gd93d8kwezvcrekdq0", "CL8Y Voting frontend"),
}


def call(method, path, payload=None):
    base = os.environ["COOLIFY_URL"].rstrip("/")
    origin = urllib.parse.urlsplit(base)
    if origin.hostname not in ("10.8.0.1", "10.10.0.2", "coolify.cl8y.com") or origin.username:
        raise RuntimeError("unrecognized control plane origin")
    lines = ["header = " + json.dumps("Authorization: Bearer " + os.environ["COOLIFY_WIRE_TOKEN"]),
             'header = "Accept: application/json"', 'header = "Content-Type: application/json"']
    if payload is not None:
        lines.append("data = " + json.dumps(json.dumps(payload)))
    result = subprocess.run(
        ["curl", "--noproxy", "*", "--silent", "--show-error", "--connect-timeout", "5",
         "--max-time", "45", "--config", "-", "--request", method,
         "--write-out", "\n%{http_code}", base + "/api/v1" + path],
        input="\n".join(lines) + "\n", capture_output=True, text=True, timeout=50)
    if result.returncode:
        raise RuntimeError("control plane transport failed")
    body, _, status = result.stdout.rpartition("\n")
    if not status.isdigit() or not 200 <= int(status) < 300:
        raise RuntimeError("control plane rejected request: HTTP " + status)
    return json.loads(body) if body else {}


def resource(kind):
    uuid, name = RESOURCES[kind]
    actual = call("GET", "/applications/" + uuid)
    if actual.get("uuid") != uuid or actual.get("name") != name:
        raise RuntimeError("voting resource identity mismatch")
    if actual.get("git_repository") != "https://git.cl8y.com/code/voting.git":
        raise RuntimeError("resource is not reading the voting Forgejo repository")
    return uuid, actual.get("id")


def validate_deployment(result, uuid, application_id):
    # This installation omits the internal id from application GET responses,
    # but includes the related application on deployment GET responses.
    related = result.get("application", {})
    if related.get("uuid") != uuid or str(result.get("application_id")) != str(related.get("id")):
        raise RuntimeError("deployment belongs to a different application")
    if application_id is not None and str(result.get("application_id")) != str(application_id):
        raise RuntimeError("deployment application identity changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("stop", "deploy", "status", "recover", "clear-recovery"))
    parser.add_argument("resource", choices=RESOURCES)
    parser.add_argument("--sha")
    parser.add_argument("--deployment")
    args = parser.parse_args()
    uuid, application_id = resource(args.resource)
    if args.action == "stop":
        call("POST", "/applications/" + uuid + "/stop")
        print(json.dumps({"resource": args.resource, "action": "stop_requested"}))
    elif args.action in ("recover", "clear-recovery"):
        if args.resource != "ledger":
            raise RuntimeError("baseline recovery is ledger-only")
        value = "true" if args.action == "recover" else "false"
        call("PATCH", "/applications/" + uuid + "/envs/bulk", {"data": [{
            "key": "LEDGER_RECONCILE_LIVE_BASELINE", "value": value,
            "is_buildtime": False, "is_runtime": True}]})
        print(json.dumps({"resource": "ledger", "recovery_configured": value == "true"}))
    elif args.action == "deploy":
        if not args.sha or not re.fullmatch(r"[0-9a-f]{40}", args.sha):
            raise RuntimeError("a complete tested main commit is required")
        main_sha = subprocess.check_output(["git", "rev-parse", "origin/main"], text=True).strip()
        remote = subprocess.check_output(["git", "remote", "get-url", "origin"], text=True).strip()
        if remote != "https://git.cl8y.com/code/voting.git":
            raise RuntimeError("release must run from the voting origin checkout")
        if args.sha != main_sha:
            raise RuntimeError("release commit differs from fetched origin/main")
        configuration = {"git_branch": "main", "git_commit_sha": args.sha}
        if args.resource == "frontend":
            # Static wrapping discards this multi-stage image's nginx runtime.
            configuration.update({"is_static": False, "is_spa": False,
                                  "build_pack": "dockerfile", "base_directory": "/",
                                  "dockerfile_location": "/deploy/docker/frontend.Dockerfile"})
        elif args.resource == "ledger":
            configuration["health_check_path"] = "/live"
        call("PATCH", "/applications/" + uuid, configuration)
        configured = call("GET", "/applications/" + uuid)
        if args.resource == "frontend" and (
            configured.get("settings", {}).get("is_static") is not False
            or configured.get("dockerfile_location") != "/deploy/docker/frontend.Dockerfile"
            or configured.get("build_pack") != "dockerfile"
        ):
            raise RuntimeError("frontend image mode was not applied")
        if args.resource == "ledger" and configured.get("health_check_path") != "/live":
            raise RuntimeError("ledger liveness probe was not applied")
        result = call("POST", "/deploy?" + urllib.parse.urlencode({"uuid": uuid, "force": "true"}))
        deployments = result.get("deployments", [])
        print(json.dumps({"resource": args.resource, "commit": args.sha, "deployments": [
            {k: item.get(k) for k in ("deployment_uuid", "resource_uuid", "resource_name")}
            for item in deployments]}))
    else:
        if not args.deployment:
            result = call("GET", "/applications/" + uuid)
            print(json.dumps({"resource": args.resource, "status": result.get("status"),
                              "configured_commit": result.get("git_commit_sha")}))
            return
        if not re.fullmatch(r"[a-zA-Z0-9]+", args.deployment):
            raise RuntimeError("an inventoried deployment ID is required")
        result = call("GET", "/deployments/" + args.deployment)
        validate_deployment(result, uuid, application_id)
        print(json.dumps({"resource": args.resource, "deployment": args.deployment,
                          **{k: result.get(k) for k in ("status", "commit", "created_at", "updated_at")}}))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Error strings from subprocesses/responses may contain secrets.
        print(json.dumps({"success": False, "error_class": type(error).__name__}))
        raise SystemExit(1)
