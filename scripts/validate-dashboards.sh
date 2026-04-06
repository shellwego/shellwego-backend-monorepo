#!/usr/bin/env bash
set -euo pipefail
DASHBOARD_DIR="${1:-config/grafana/dashboards}"
ERRORS=0
for f in "$DASHBOARD_DIR"/*.json; do
    if ! python3 -m json.tool "$f" > /dev/null 2>&1; then
        echo "INVALID JSON: $f"
        ERRORS=$((ERRORS + 1))
    else
        # Check required fields
        if ! python3 -c "
import json, sys
d = json.load(open('$f'))
assert 'uid' in d, 'missing uid'
assert 'panels' in d, 'missing panels'
assert len(d['panels']) > 0, 'no panels'
for p in d.get('panels', []):
    if 'targets' in p:
        for t in p['targets']:
            assert 'expr' in t, f'panel {p.get(\"title\",\"?\")} missing expr'
"; then
            echo "INVALID DASHBOARD: $f"
            ERRORS=$((ERRORS + 1))
        fi
    fi
done
if [ $ERRORS -eq 0 ]; then
    echo "All dashboards valid."
else
    echo "$ERRORS dashboard(s) invalid."
    exit 1
fi
