#!/usr/bin/env bash
#
# Provision Elasticsearch for StellarLend event storage (issue #685):
# ILM policy, index template, and the first index behind the
# `stellarlend-events` write alias. Idempotent — safe to re-run.
#
#   ELASTICSEARCH_URL=http://localhost:9200 ./setup.sh
#   ELASTICSEARCH_API_KEY=... (optional)
#
set -euo pipefail

ES="${ELASTICSEARCH_URL:-http://localhost:9200}"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUTH=()
[[ -n "${ELASTICSEARCH_API_KEY:-}" ]] && AUTH=(-H "Authorization: ApiKey ${ELASTICSEARCH_API_KEY}")

es() {
    local method=$1 path=$2 body=${3:-}
    curl -sS --fail-with-body -X "$method" "${ES}${path}" "${AUTH[@]}" \
        -H 'Content-Type: application/json' ${body:+--data-binary "@${body}"}
    echo
}

echo "→ ILM policy stellarlend-events"
es PUT /_ilm/policy/stellarlend-events "${DIR}/ilm-policy.json"

echo "→ index template stellarlend-events"
es PUT /_index_template/stellarlend-events "${DIR}/index-template.json"

if curl -sS -o /dev/null -w '%{http_code}' "${AUTH[@]}" "${ES}/_alias/stellarlend-events" | grep -q '^200$'; then
    echo "→ write alias stellarlend-events already exists"
else
    echo "→ bootstrap index stellarlend-events-000001"
    tmp=$(mktemp)
    echo '{"aliases":{"stellarlend-events":{"is_write_index":true}}}' > "$tmp"
    es PUT /stellarlend-events-000001 "$tmp"
    rm -f "$tmp"
fi

echo "Done. Point the API at it with ELASTICSEARCH_URL=${ES}"
