#!/bin/sh
# Builds the Rust services from this checkout and restarts them. Run on the box, as root, after
# pulling main. The web app and the Worker are not deployed here: Cloudflare Workers Builds does
# that on every push to main (docs/architecture.md).
set -eu
cd "$(dirname "$0")/.."
cargo build --release
for unit in infra/systemd/*.service; do
  install -m 644 "$unit" /etc/systemd/system/
done
systemctl daemon-reload
for bin in aave-watch-api aave-watch-indexer; do
  # Install beside, then rename: a running service keeps its old file until it restarts.
  install -m 755 "target/release/$bin" "/usr/local/bin/$bin.new"
  mv "/usr/local/bin/$bin.new" "/usr/local/bin/$bin"
done
./infra/clickhouse/migrate.sh
/usr/local/bin/aave-watch-api load-registry
systemctl enable --now aave-watch-api aave-watch-fetch aave-watch-process
systemctl restart aave-watch-api aave-watch-fetch aave-watch-process
systemctl --no-pager --lines=0 status aave-watch-api aave-watch-fetch aave-watch-process | grep -E "●|Active:"
