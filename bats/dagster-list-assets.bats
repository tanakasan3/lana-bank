#!/usr/bin/env bats

load helpers

# Helper to check if BigQuery credentials are available
has_bigquery_credentials() {
  [[ -n "${SA_CREDS_BASE64:-}" ]]
}

@test "dagster: list all materializable assets" {
  if [[ "${DAGSTER}" != "true" ]]; then
    skip "Skipping dagster tests"
  fi
  if ! has_bigquery_credentials; then
    skip "Skipping - requires BigQuery credentials for code location to load"
  fi

  exec_dagster_graphql "assets"
  dagster_validate_json || return 1

  echo ""
  echo "=== All Dagster Materializable Assets ==="
  echo ""

  # Get total count
  total_count=$(echo "$output" | jq -r '.data.assetsOrError.nodes | length')
  echo "Total assets: $total_count"
  echo ""

  # List assets grouped by prefix (first element of path)
  echo "--- Assets by group ---"
  echo ""

  # Get unique prefixes and iterate
  prefixes=$(echo "$output" | jq -r '.data.assetsOrError.nodes[]?.key.path[0] // "unknown"' | sort -u)

  for prefix in $prefixes; do
    echo "[$prefix]"
    echo "$output" | jq -r --arg prefix "$prefix" \
      '.data.assetsOrError.nodes[]?.key.path | select(.[0] == $prefix) | "  " + (. | join("/"))' | sort
    echo ""
  done

  echo "=== End of Asset List ==="
}
