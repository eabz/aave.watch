#!/bin/sh
# Applies the numbered .sql files in this folder, in order, once each.
# usage: ./migrate.sh [clickhouse-client args, e.g. --host 127.0.0.1 --port 9000]
set -eu
cd "$(dirname "$0")"
ch() { clickhouse-client "$@"; }
ch "$@" --multiquery --query "
  CREATE DATABASE IF NOT EXISTS meta;
  CREATE TABLE IF NOT EXISTS meta.schema_migrations (name String, applied_at DateTime DEFAULT now())
  ENGINE = MergeTree ORDER BY name;"
for f in [0-9]*.sql; do
  if [ "$(ch "$@" --query "SELECT count() FROM meta.schema_migrations WHERE name = '$f'")" = "0" ]; then
    echo "applying $f"
    ch "$@" --multiquery < "$f"
    ch "$@" --query "INSERT INTO meta.schema_migrations (name) VALUES ('$f')"
  fi
done
echo "schema is up to date"
