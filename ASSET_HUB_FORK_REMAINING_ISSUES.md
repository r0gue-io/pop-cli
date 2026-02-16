# Asset Hub Fork + Foundry: Remaining Issue Status

Date: 2026-02-16
Branch: `fix/contract-prompts-stacked-pr948-v2`
Last implementation commit: `af1e4851`

> Status: WIP. This document is the authoritative context for remaining work.

## Goal

Make this flow work end-to-end on a **local fork of Asset Hub**:

1. `pop fork --endpoint <asset-hub> --dev`
2. use emitted local eth-rpc endpoint
3. run Foundry deploy + interact (`forge create`, `cast send/call`)

## What Is Already Working

- `pop fork` starts successfully against Asset Hub Paseo (example endpoint: `wss://sys.ibp.network/asset-hub-paseo`).
- `--dev` funding executes.
- Local fork endpoint is healthy (example: `ws://127.0.0.1:9946`).
- eth-rpc bridge starts and endpoint is emitted (example: `ws://127.0.0.1:8546`).
- Basic eth-rpc checks work (`cast chain-id`, `cast balance`).

## Remaining Blocker

Foundry deployment still fails on the local eth-rpc endpoint with:

`server returned an error response: error code -32000: Metadata error: The generated code is not compatible with the node`

This blocks deploy and therefore also blocks interact flow.

## Exact Reproduction (Confirmed)

### 1) Start fork

```bash
../pop-cli/target/debug/pop fork \
  --endpoint wss://sys.ibp.network/asset-hub-paseo \
  --dev \
  --port 9946 \
  --eth-rpc-port 8546
```

Expected startup output includes:

- `Forked asset-hub-paseo ... -> ws://127.0.0.1:9946`
- `eth rpc: ws://127.0.0.1:8546`

### 2) Sanity check eth-rpc

```bash
cast chain-id --rpc-url ws://127.0.0.1:8546
```

Returns chain id successfully.

### 3) Foundry deploy

```bash
cd /tmp/pop-e2e-flow
forge create Counter --resolc \
  --rpc-url ws://127.0.0.1:8546 \
  --private-key 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133 \
  --broadcast \
  --constructor-args 0
```

Observed failure:

`Metadata error: The generated code is not compatible with the node`

## What Was Implemented Already (and Verified)

- Added asset-hub eth-rpc startup retry/fallback policy in `pop-contracts`.
- Added classification utilities for startup/runtime error strings.
- Added run options support (`EthRpcRunOptions`) and argument builder.
- Added attempts sequence: default + `--chain=staging` (+ `--no-prometheus`).
- Wired fallback logic in:
  - `crates/pop-cli/src/commands/fork.rs`
  - `crates/pop-cli/src/commands/up/network.rs`
- Added unit tests for classification, retry policy, and attempt selection.

Result: startup robustness improved, but deploy mismatch remains.

## Extra Verification Already Done

- Manually launched eth-rpc with explicit staging profile:

```bash
/Users/R0GUE/Library/Caches/pop/eth-rpc-v0.47.0 \
  --node-rpc-url=ws://127.0.0.1:9946 \
  --rpc-port=8547 \
  --chain=staging \
  --no-prometheus
```

- Re-ran deploy against `ws://127.0.0.1:8547`.
- Same metadata incompatibility error persisted.

Conclusion: this is not just a startup argument issue.

## Current Root-Cause Hypothesis

Most likely remaining issue is in **deploy-path runtime compatibility** between `eth-rpc` and the forked runtime context, not bridge startup.

Practical interpretation:

- eth-rpc can connect and serve basic RPC calls,
- but when Foundry triggers deploy-related calls, eth-rpc hits runtime metadata/API incompatibility in that path.

## Why Foundry-Polkadot Code Does Not Automatically Solve This Fork Case

`foundry-polkadot` revive/create tests show two working modes:

1. live chain eth-rpc endpoint (westend/passet hub), or
2. local `substrate-node` + `eth-rpc` pair built to be compatible.

That does not directly prove local **forked** Asset Hub compatibility without matching runtime/API behavior in fork server path.

## Highest-Value Next Step

Instrument and identify the **exact runtime API call** failing during deploy path, then patch that path specifically.

Suggested minimal sequence:

1. Run fork + eth-rpc with focused debug logs and capture the first failing call during `forge create`.
2. Map failing method to fork server handling (`state_call` proxy/local execution path).
3. Implement targeted compatibility fix and verify with one full deploy+interact run.

## Demo Fallback (If Needed Today)

For demo reliability, deploy/interact against a known live Asset Hub eth-rpc endpoint, and use local fork only for read-side/dev-account demonstrations until this deploy-path mismatch is fixed.

## Avoid Wasting Time On

- More startup retry permutations.
- More `cast chain-id`/basic connectivity checks.
- Repeating broad research loops without isolating the failing runtime method in deploy path.
