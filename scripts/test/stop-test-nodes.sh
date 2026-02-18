#!/usr/bin/env bash
# Kills test nodes spawned by spawn-test-nodes.sh.

set -euo pipefail

if [[ "$(uname -s)" == "Darwin" ]]; then
  PID_FILE="${HOME}/Library/Caches/pop/test-nodes/pids"
else
  PID_FILE="${HOME}/.cache/pop/test-nodes/pids"
fi

if [[ ! -f "${PID_FILE}" ]]; then
  echo "No PID file found. Nothing to kill." >&2
  exit 0
fi

killed=0
while IFS= read -r pid; do
  if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null && echo "Killed node (pid ${pid})" >&2
    killed=$((killed + 1))
  fi
done < "${PID_FILE}"

: > "${PID_FILE}"

if [[ ${killed} -eq 0 ]]; then
  echo "No running test nodes found." >&2
else
  echo "Stopped ${killed} node(s)." >&2
fi
