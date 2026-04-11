#!/bin/bash
# Start the NEBULA admin dashboard (Linux desktop or Chrome)
# Usage: ./scripts/dev-admin.sh [linux|chrome|macos|windows]

set -euo pipefail
cd "$(dirname "$0")/../nebula-admin"

DEVICE="${1:-linux}"

flutter pub get
echo "Starting NEBULA admin dashboard on $DEVICE..."
echo "  Server URL: http://localhost:8080 (default, change in settings)"
echo ""

exec flutter run -d "$DEVICE"
