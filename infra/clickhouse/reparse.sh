#!/bin/sh
# Reparses everything: the processor walks raw.* again from each chain's first block and rewrites
# core.* and meta.assets, reserves, tokens. Run after a parser change (bump PARSER_VERSION first)
# or after logs were added to raw at old blocks. No node is involved.
# Nothing is deleted: every table is a ReplacingMergeTree on its natural key, so rows are replaced
# as the walk passes them and the API keeps answering meanwhile. Rows the new parser no longer
# writes keep their old parser_version; delete them by that column if they matter.
# Arguments pass through to clickhouse-client.
set -eu
cd "$(dirname "$0")"
systemctl stop aave-watch-process
clickhouse-client "$@" --query "ALTER TABLE meta.cursor DELETE WHERE stream = 'core' SETTINGS mutations_sync = 2"
systemctl start aave-watch-process
echo "reparse started. When the core cursor has caught up with the calls cursor, run ./rollup_backfill.sh"
