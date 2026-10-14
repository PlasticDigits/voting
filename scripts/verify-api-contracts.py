#!/usr/bin/env python3
"""Validate captured isolated HTTP responses against generated OpenAPI schemas."""
import argparse
import json
from pathlib import Path
import re
import sys

from jsonschema import Draft202012Validator, FormatChecker


def verify(spec, captures, require_success=False):
    checked = 0
    failures = []
    covered = set()
    for capture in captures:
        path = capture["path"].split("?", 1)[0]
        method = capture["method"].lower()
        matches = [(template, operations[method]) for template, operations in spec["paths"].items()
                   if method in operations and re.fullmatch(re.sub(r"\{[^}]+\}", "[^/]+", template), path)]
        if len(matches) != 1:
            failures.append({"path": path, "method": method, "error": "undocumented operation"})
            continue
        template, operation = matches[0]
        status = str(capture["status"])
        response = operation.get("responses", {}).get(status, {})
        node = response.get("content", {}).get("application/json", {}).get("schema")
        if node is None:
            failures.append({"route": template, "method": method, "status": status,
                             "error": "undocumented JSON response"})
            continue
        schema = {**node, "components": spec.get("components", {})}
        validator = Draft202012Validator(schema, format_checker=FormatChecker())
        errors = list(validator.iter_errors(capture["body"]))
        # Emit structural failure locations, never arbitrary bodies or messages.
        for error in errors:
            failures.append({"route": template, "method": method, "status": status,
                             "field": "/".join(map(str, error.absolute_path)),
                             "validator": error.validator})
        checked += 1
        if not errors:
            covered.add((template, method, status))
    if require_success:
        successes = {(path, method) for path, method, status in covered if status.startswith("2")}
        for path, operations in spec["paths"].items():
            for method in ("get", "post", "put"):
                if method in operations and (path, method) not in successes:
                    failures.append({"route": path, "method": method, "error": "missing successful response coverage"})
    return {"passed": checked > 0 and not failures, "responses_checked": checked,
            "covered": [list(item) for item in sorted(covered)], "failures": failures}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=Path)
    parser.add_argument("--spec", type=Path, default=Path("protocol/openapi.json"))
    parser.add_argument("--require-success", action="store_true")
    args = parser.parse_args()
    spec = json.loads(args.spec.read_text())
    captures = [json.loads(line) for line in args.captures.read_text().splitlines() if line.strip()]
    result = verify(spec, captures, args.require_success)
    print(json.dumps(result, indent=2))
    sys.exit(0 if result["passed"] else 1)
