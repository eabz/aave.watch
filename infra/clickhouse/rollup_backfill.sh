#!/bin/sh
# Recomputes every rollup over all of core.*, not only the last hours the refreshable views cover.
# Run after a backfill or a reparse of old data. Safe to repeat: the targets are ReplacingMergeTree
# on the bucket key. Arguments pass through to clickhouse-client.
# The statements are the views' own SELECTs from 0007_core_rollups.sql, without their time filter.
set -eu
cd "$(dirname "$0")"
awk '
  /^CREATE MATERIALIZED VIEW/ { view = 1; next }
  view && /^REFRESH / { sub(/^REFRESH .* TO /, "INSERT INTO "); sub(/ AS$/, ""); print; next }
  view && /^WHERE block_time >= / { next }
  view { print }
  view && /;$/ { view = 0 }
' [0-9]*.sql | clickhouse-client "$@" --multiquery
