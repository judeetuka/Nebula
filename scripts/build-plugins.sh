#!/bin/bash
# Build all plugin .so binaries for Android (aarch64).
# Usage: ./scripts/build-plugins.sh

set -euo pipefail

ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-/home/iam/devcode/projects/experiments/development}"
NDK_VERSION="${NDK_VERSION:-29.0.14206865}"
ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/$NDK_VERSION"
TARGET="aarch64-linux-android"
PROFILE="${1:-release}"

export PATH="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="aarch64-linux-android21-clang"

echo "Building plugins for $TARGET ($PROFILE profile)"
echo "NDK: $ANDROID_NDK_HOME"
echo ""

PLUGIN_DIRS=(
    plugins/built-in/device-info
    plugins/built-in/observer
    plugins/built-in/comm-link
    plugins/built-in/accessibility
    plugins/built-in/action-mirror
    plugins/built-in/file-access
    plugins/first-party/browser
    plugins/first-party/classifier
    plugins/first-party/contacts
    plugins/first-party/email
    plugins/first-party/payment-processor
    plugins/first-party/linux-bridge
    plugins/first-party/screen-stream
)

BUILT=0
FAILED=0

for dir in "${PLUGIN_DIRS[@]}"; do
    name=$(basename "$dir")
    echo -n "  Building $name... "
    if cargo build --manifest-path "$dir/Cargo.toml" --target "$TARGET" --"$PROFILE" 2>/dev/null; then
        echo "OK"
        BUILT=$((BUILT + 1))
    else
        echo "FAILED"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
echo "Done: $BUILT built, $FAILED failed (out of ${#PLUGIN_DIRS[@]})"
echo ""
if [ "$PROFILE" = "release" ]; then
    echo "Binaries at: target/$TARGET/release/*.so"
else
    echo "Binaries at: target/$TARGET/debug/*.so"
fi
