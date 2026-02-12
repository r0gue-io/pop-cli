---
name: pop-cli
description: Use Pop CLI (`pop`) for Polkadot SDK chain and ink! contract workflows. Use when scaffolding, building, testing, launching nodes or networks, deploying contracts, or calling chain and contract methods. Enforce command-only execution and reliable cleanup.
---

# Pop CLI

Use this skill for terminal workflows driven by `pop`.

## Hard Rules

- Run plain `pop` commands only, one command per step.
- Never chain commands or use shell wrappers/operators (`&&`, `;`, `|`, command substitution, scripts/functions).
- Exception: env expansion is allowed only for signer secret on `--suri` (example: `--suri "$PRIVATE_KEY"`).
- Prefer explicit flags and inputs over prompts (`--path`, `--url`, `--args`, `--skip-confirm` where supported).
- Never print secrets.

## Cleanup Rules

After every flow, clean local processes and networks with `pop clean`:

- `pop clean node --all`
- `pop clean node --pid <PID>...`
- `pop clean network --all`
- `pop clean network <base-dir-or-zombie.json>`

## Reliability Rules for Call Flows

For `pop call chain` and `pop call contract`, evaluate both:

- Process exit code
- Output markers in stdout and stderr

Treat output containing these markers as failure, even if exit code is `0`:

- `Failed to encode call data`
- `Failed to query storage`
- `RPC error`
- `Error:`

## Tested Gotchas

- `pop new contract` does not accept `-y`.
- `pop new chain` does not accept `-y`.
- `pop up <contract-dir> --skip-confirm` requires all constructor args via `--args`.
- `pop call contract` requires `--gas` and `--proof-size` together; passing only one fails argument validation.
- `pop call chain --function` expects runtime/source naming and commonly requires `snake_case` (example: `transfer_keep_alive`).
- For calls expecting `MultiAddress` (example: `Balances.transfer_keep_alive`), pass destination as `Id(0x...)`.
- `pop up <contract-dir>` may own local node lifecycle and terminate it; for stable multi-step flows use explicit non-default node ports and explicit `--url`.
- `pop up network --detach` can report running before endpoints are reachable; verify liveness with an immediate read call.
- `pop up network --cmd` executes one command using whitespace splitting (`split_whitespace`), not a shell.
- `pop up <path>` checks only the provided path; it does not recurse into subdirectories.

## Command Patterns

Contracts:

- `pop new contract`
- `pop build --path <contract-dir>`
- `pop test --path <contract-dir>`
- `pop up ink-node`
- `pop up <contract-dir> --constructor <name> --args <arg>... --suri "$PRIVATE_KEY" --url <ws-url> --skip-confirm`
- `pop call contract --path <contract-dir> --contract 0x... --message <name> --args <arg>... --suri "$PRIVATE_KEY" --url <ws-url> --skip-confirm`

Chains:

- `pop new chain`
- `pop build --path <chain-dir>`
- `pop test --path <chain-dir>`
- `pop up network <zombie.json> --detach`
- `pop call chain --url <ws-url> --pallet <pallet> --function <snake_case_name> --args <arg>... --suri "$PRIVATE_KEY" --skip-confirm`

Utilities:

- `pop convert address --address <value>`
- `pop clean node --all`
- `pop clean network --all`

## Completion Checklist

- Each step used exactly one `pop` command.
- No shell chaining/operator usage was introduced.
- Call-flow success considered both exit code and output markers.
- Cleanup completed with `pop clean node` and/or `pop clean network`.
