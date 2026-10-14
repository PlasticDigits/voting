#!/usr/bin/env python3
"""Build the current frontend image, then prove each invalid configuration fails its guard."""
import json
from pathlib import Path
import subprocess
import sys
import time

import argparse
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir', type=Path)
options = parser.parse_args()
ROOT = Path(__file__).resolve().parents[1]
OUT = options.output_dir or Path(tempfile.mkdtemp(prefix='voting-docker-guards-'))
OUT.mkdir(parents=True, exist_ok=True)
VALID = {'VITE_OPERATOR_VOTING_URL': 'https://operator.vote.cl8y.com',
         'VITE_WC_PROJECT_ID': '00000000000000000000000000000000'}
CASES = [
    ('missing_api', 'VITE_OPERATOR_VOTING_URL', '', None),
    ('insecure_api', 'VITE_OPERATOR_VOTING_URL', 'http://api.example', 'VITE_OPERATOR_VOTING_URL must be https://'),
    ('credentials_api', 'VITE_OPERATOR_VOTING_URL', 'https://fixture-user:fixture-password@api.example',
     'VITE_OPERATOR_VOTING_URL must be a public HTTPS URL without credentials, query or fragment.'),
    ('missing_walletconnect', 'VITE_WC_PROJECT_ID', '', None),
    ('local_network', 'VITE_NETWORK', 'local', 'VITE_NETWORK must be mainnet for production builds.'),
    ('legal_hatch', 'VITE_PLAYWRIGHT_E2E', '1', None),
    ('dev_mnemonic', 'VITE_DEV_MNEMONIC', 'sentinel-test-fixture', None),
    ('browser_bsc_rpc', 'VITE_BSC_RPC', 'https://bsc.example', None),
    ('browser_bsc_rpc_url', 'VITE_BSC_RPC_URL', 'https://bsc.example', None),
]
baseline = ['docker', 'build', '--progress=plain', '-f', 'deploy/docker/frontend.Dockerfile']
for key, value in VALID.items():
    baseline.extend(['--build-arg', f'{key}={value}'])
baseline.extend(['-t', 'voting-frontend-guard-fixture:valid', '.'])
with (OUT / 'valid.log').open('w') as output:
    valid = subprocess.run(baseline, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
if valid.returncode:
    print(json.dumps({'passed': False, 'failure': 'valid baseline build failed', 'log': str(OUT / 'valid.log')}))
    sys.exit(1)
rows = []
for name, key, value, diagnostic in CASES:
    args = {**VALID, key: value}
    command = ['docker', 'build', '--progress=plain', '-f', 'deploy/docker/frontend.Dockerfile']
    for arg, setting in args.items():
        command.extend(['--build-arg', f'{arg}={setting}'])
    command.extend(['-t', f'voting-frontend-guard-fixture:{name}', '.'])
    start = time.monotonic()
    logpath = OUT / f'{name}.log'
    with logpath.open('w') as output:
        result = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
    log = logpath.read_text()
    # Guard RUN failures are distinct from COPY/pull/npm-ci/network failures.
    guard_step = 'ERROR: process "/bin/sh -c test -n' in log
    started_build = '> @cl8y/voting-frontend@0.1.0 build' in log
    if diagnostic:
        recognized = guard_step and diagnostic in log
        reason = diagnostic
    else:
        # Each case differs from the successfully built baseline by exactly one ARG.
        # Early guards are silent: prove rejection at the named guard RUN, before npm build.
        recognized = guard_step and key in log and not started_build
        reason = f'{key}: rejected by Dockerfile shell guard before npm run build'
    row = {'case': name, 'argument': key, 'exit_code': result.returncode,
           'rejected_by_guard': result.returncode != 0 and recognized,
           'recognizable_reason': reason if recognized else None,
           'guard_step': guard_step, 'npm_build_started': started_build,
           'duration_seconds': round(time.monotonic() - start, 2), 'log': str(logpath)}
    rows.append(row)
    print(json.dumps(row), flush=True)
summary = {'valid_image': 'voting-frontend-guard-fixture:valid',
           'passed': all(row['rejected_by_guard'] for row in rows), 'cases': rows}
(OUT / 'matrix.json').write_text(json.dumps(summary, indent=2) + '\n')
sys.exit(0 if summary['passed'] else 1)
