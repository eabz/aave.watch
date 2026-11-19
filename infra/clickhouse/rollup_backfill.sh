#!/bin/sh
# Recomputes every rollup over all of core.*, not only the last hours the refreshable views cover.
# Run after a backfill or a reparse of old data. Safe to repeat: the targets are ReplacingMergeTree
# on the bucket key. Arguments pass through to clickhouse-client.
# The statements are the views' own SELECTs, without their time filter. A view takes part when its
# target is named on a line of its own (`REFRESH ... TO x AS` or `TO x AS`); the *_latest views of
# 0006 do not, and need no refill.
set -eu
cd "$(dirname "$0")"
awk '
  /^CREATE MATERIALIZED VIEW/ { view = 1; fill = 0; next }
  view && /^(REFRESH .* )?TO / { sub(/^(REFRESH .* )?TO /, "INSERT INTO "); sub(/ AS$/, ""); fill = 1; print; next }
  view && /^WHERE block_time >= / { next }
  view && fill { print }
  view && /;$/ { view = 0 }
' [0-9]*.sql | clickhouse-client "$@" --multiquery
