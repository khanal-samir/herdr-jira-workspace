#!/bin/sh
set -eu
bin="${HERDR_BIN_PATH:-herdr}"
entrypoint="${2:-workspace}"
if "$bin" plugin pane focus --plugin herdr-jira-workspace --entrypoint "$entrypoint" >/dev/null 2>&1; then
    exit 0
fi
exec "$bin" plugin pane open --plugin herdr-jira-workspace --entrypoint "$entrypoint" --placement "${1:-tab}" --focus
