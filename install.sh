#!/bin/bash
# install.sh - Installer for pop-cli

# Users can install pop-cli with this script by running:
# curl -s https://raw.githubusercontent.com/r0gue-io/pop-cli/main/install.sh | bash

set -e

# Configuration
REPO="r0gue-io/pop-cli"
PACKAGE_NAME="pop"
INSTALL_DIR="${HOME}/.local/bin"

# Detect OS
OS=$(uname -s)
case "$OS" in
    Linux*)     OS_TYPE="unknown-linux-gnu";;
    Darwin*)    OS_TYPE="apple-darwin";;
    *)
        echo "❌ Unsupported operating system: $OS"
        exit 1
        ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)     ARCH_TYPE="x86_64";;
    aarch64|arm64)  ARCH_TYPE="aarch64";;
    *)
        echo "❌ Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Construct target triple
TARGET="${ARCH_TYPE}-${OS_TYPE}"

echo "Detected target: $TARGET"

# Get latest release
echo "Fetching latest release..."
LATEST_RELEASE=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_RELEASE" ]; then
    echo "❌ Failed to fetch latest release"
    exit 1
fi

echo "Latest version: ${LATEST_RELEASE}"

# Construct download URL
PACKAGE="${PACKAGE_NAME}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_RELEASE}/${PACKAGE}"

# Download and extract
echo "Downloading ${PACKAGE}..."
TMP_DIR=$(mktemp -d)
cd "$TMP_DIR"

if ! curl -L -f -o "${PACKAGE}" "${DOWNLOAD_URL}"; then
    echo "❌ Failed to download ${PACKAGE}"
    echo "URL: ${DOWNLOAD_URL}"
    rm -rf "$TMP_DIR"
    exit 1
fi

echo "Extracting binary..."
tar -xzf "${PACKAGE}"

# Create install directory if it doesn't exist
mkdir -p "${INSTALL_DIR}"

# Install binary
echo "Installing ${PACKAGE_NAME} to ${INSTALL_DIR}..."
mv pop "${INSTALL_DIR}/"
chmod +x "${INSTALL_DIR}/pop"

# Cleanup
cd - > /dev/null
rm -rf "$TMP_DIR"

echo "✅ ${PACKAGE_NAME} ${LATEST_RELEASE} installed successfully to ${INSTALL_DIR}/pop"

# Check if install directory is in PATH
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "⚠️ ${INSTALL_DIR} is not in your PATH."
    echo "   Add it to your PATH by running:"
    echo ""
    echo "   export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    echo "   To make this permanent, add the above line to your shell profile:"
    echo "   (~/.bashrc, ~/.zshrc, or ~/.profile)"
else
    echo ""
    echo "Run 'pop --version' to verify the installation."
fi
