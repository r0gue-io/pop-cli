#!/usr/bin/env bash
# Spawns ink-node and substrate-node for nextest setup scripts.
# Writes POP_TEST_INK_NODE_WS_URL and POP_TEST_SUBSTRATE_NODE_WS_URL to $NEXTEST_ENV
# so every test process inherits them without spawning its own node.
#
# Skips a node if its env var is already set (e.g. CI pre-starts the node).
# Kills leftover nodes from previous runs before spawning new ones.
# A background watchdog automatically kills nodes when nextest exits.

set -euo pipefail

# ── Versions (keep in sync with crates/pop-common/src/test_env.rs) ──────────
INK_NODE_TAG="v0.47.0"
SUBSTRATE_NODE_TAG="polkadot-stable2512-1"

# ── Platform detection ──────────────────────────────────────────────────────
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)
    INK_ARCHIVE="ink-node-mac-universal.tar.gz"
    INK_BIN_REL="ink-node-mac/ink-node"
    SUBSTRATE_ARCHIVE="substrate-node-aarch64-apple-darwin.tar.gz"
    ;;
  Darwin-x86_64)
    INK_ARCHIVE="ink-node-mac-universal.tar.gz"
    INK_BIN_REL="ink-node-mac/ink-node"
    SUBSTRATE_ARCHIVE="substrate-node-x86_64-apple-darwin.tar.gz"
    ;;
  Linux-x86_64)
    INK_ARCHIVE="ink-node-linux.tar.gz"
    INK_BIN_REL="ink-node-linux/ink-node"
    SUBSTRATE_ARCHIVE="substrate-node-x86_64-unknown-linux-gnu.tar.gz"
    ;;
  Linux-aarch64)
    INK_ARCHIVE="ink-node-linux.tar.gz"
    INK_BIN_REL="ink-node-linux/ink-node"
    SUBSTRATE_ARCHIVE="substrate-node-aarch64-unknown-linux-gnu.tar.gz"
    ;;
  *)
    echo "Unsupported platform: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

# ── Cache directory ─────────────────────────────────────────────────────────
if [[ "$(uname -s)" == "Darwin" ]]; then
  CACHE_DIR="${HOME}/Library/Caches/pop/test-nodes"
else
  CACHE_DIR="${HOME}/.cache/pop/test-nodes"
fi
mkdir -p "${CACHE_DIR}"

PID_FILE="${CACHE_DIR}/pids"

# ── Kill leftover nodes from previous runs ──────────────────────────────────
if [[ -f "${PID_FILE}" ]]; then
  while IFS= read -r pid; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      echo "Killed leftover node (pid ${pid})" >&2
    fi
  done < "${PID_FILE}"
fi
: > "${PID_FILE}"

# ── Helpers ─────────────────────────────────────────────────────────────────
find_free_port() {
  python3 -c '
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
'
}

wait_for_rpc() {
  local url="$1" max_attempts="${2:-30}"
  local payload='{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}'
  for _ in $(seq 1 "${max_attempts}"); do
    if curl -s -X POST -H 'Content-Type: application/json' -d "${payload}" "${url}" \
         2>/dev/null | grep -q '"result"'; then
      return 0
    fi
    sleep 1
  done
  echo "Node at ${url} did not become ready after ${max_attempts}s" >&2
  return 1
}

download_if_missing() {
  local url="$1" dest="$2"
  if [[ ! -f "${dest}" ]]; then
    echo "Downloading ${url} ..." >&2
    local tmp
    tmp="$(mktemp)"
    curl --fail --show-error --silent --location --retry 3 --output "${tmp}" "${url}"
    mv "${tmp}" "${dest}"
  fi
}

# Collect PIDs of nodes we spawn (for the watchdog).
NODE_PIDS=()

# ── Spawn ink-node ──────────────────────────────────────────────────────────
if [[ -z "${POP_TEST_INK_NODE_WS_URL:-}" ]]; then
  INK_CACHE="${CACHE_DIR}/ink-node/${INK_NODE_TAG}"
  mkdir -p "${INK_CACHE}"

  INK_BIN="${INK_CACHE}/${INK_BIN_REL}"
  if [[ ! -x "${INK_BIN}" ]]; then
    INK_TARBALL="${INK_CACHE}/${INK_ARCHIVE}"
    download_if_missing \
      "https://github.com/use-ink/ink-node/releases/download/${INK_NODE_TAG}/${INK_ARCHIVE}" \
      "${INK_TARBALL}"
    tar -xzf "${INK_TARBALL}" -C "${INK_CACHE}"
    chmod +x "${INK_BIN}"
  fi

  INK_PORT="$(find_free_port)"
  "${INK_BIN}" --dev --rpc-port="${INK_PORT}" --tmp >/dev/null 2>&1 &
  INK_PID=$!
  echo "${INK_PID}" >> "${PID_FILE}"
  NODE_PIDS+=("${INK_PID}")

  wait_for_rpc "http://127.0.0.1:${INK_PORT}"
  echo "POP_TEST_INK_NODE_WS_URL=ws://127.0.0.1:${INK_PORT}" >> "${NEXTEST_ENV}"
  echo "ink-node running on port ${INK_PORT} (pid ${INK_PID})" >&2
else
  echo "ink-node: using existing POP_TEST_INK_NODE_WS_URL=${POP_TEST_INK_NODE_WS_URL}" >&2
fi

# ── Spawn substrate-node ────────────────────────────────────────────────────
if [[ -z "${POP_TEST_SUBSTRATE_NODE_WS_URL:-}" ]]; then
  SUB_CACHE="${CACHE_DIR}/substrate-node/${SUBSTRATE_NODE_TAG}"
  mkdir -p "${SUB_CACHE}"

  SUB_BIN="${SUB_CACHE}/substrate-node"
  if [[ ! -x "${SUB_BIN}" ]]; then
    SUB_TARBALL="${SUB_CACHE}/${SUBSTRATE_ARCHIVE}"
    download_if_missing \
      "https://github.com/r0gue-io/polkadot/releases/download/${SUBSTRATE_NODE_TAG}/${SUBSTRATE_ARCHIVE}" \
      "${SUB_TARBALL}"
    tar -xzf "${SUB_TARBALL}" -C "${SUB_CACHE}"
    chmod +x "${SUB_BIN}"
  fi

  SUB_PORT="$(find_free_port)"
  "${SUB_BIN}" --dev --rpc-port="${SUB_PORT}" --tmp >/dev/null 2>&1 &
  SUB_PID=$!
  echo "${SUB_PID}" >> "${PID_FILE}"
  NODE_PIDS+=("${SUB_PID}")

  wait_for_rpc "http://127.0.0.1:${SUB_PORT}"
  echo "POP_TEST_SUBSTRATE_NODE_WS_URL=ws://127.0.0.1:${SUB_PORT}" >> "${NEXTEST_ENV}"
  echo "substrate-node running on port ${SUB_PORT} (pid ${SUB_PID})" >&2
else
  echo "substrate-node: using existing POP_TEST_SUBSTRATE_NODE_WS_URL=${POP_TEST_SUBSTRATE_NODE_WS_URL}" >&2
fi

# ── Watchdog: kill nodes when nextest exits ─────────────────────────────────
# $PPID is the nextest process that invoked this setup script.
# The watchdog ignores signals so it survives Ctrl+C, then cleans up once
# nextest is gone.
if [[ ${#NODE_PIDS[@]} -gt 0 ]]; then
  NEXTEST_PID="${PPID}"
  (
    trap '' INT TERM
    while kill -0 "${NEXTEST_PID}" 2>/dev/null; do
      sleep 2
    done
    for pid in "${NODE_PIDS[@]}"; do
      kill "${pid}" 2>/dev/null || true
    done
    : > "${PID_FILE}"
  ) &
  disown
fi
