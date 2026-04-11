#!/bin/bash
# Start the NEBULA dev server with SQLite
# Usage: ./scripts/dev-server.sh

set -euo pipefail
cd "$(dirname "$0")/.."

# Build if binary doesn't exist
if [ ! -f target/release/nebula-server ]; then
	echo "Building nebula-server (release)..."
	cargo build -p nebula-server --release
fi

export DATABASE_URL="sqlite://nebula_dev.db?mode=rwc"
export JWT_SECRET="dev-testing-secret-2026"
export RUST_LOG="info,nebula_server=debug"

echo "Starting NEBULA server..."
echo "  API:    http://localhost:8080"
echo "  Tunnel: 0.0.0.0:2333"
echo "  Health: http://localhost:8080/api/health"
echo ""
echo "Press Ctrl+C to stop"
echo ""

exec ./target/release/nebula-server --config config/server.development.toml
