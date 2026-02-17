---
name: pop-cli
description: Use when running `pop` to develop or interact with ink! smart contracts and Polkadot SDK chains.
---

# Pop CLI Skill

Use this skill for workflows that run through `pop`.

Run `pop --help` for getting an overview.

## Global Rules

- Run plain `pop` commands only, one command per step.
- Never chain commands or use shell wrappers/operators (`&&`, `;`, `|`, command substitution, scripts/functions).
- Exception: env expansion is allowed only for signer secret on `--suri` (example: `--suri "$PRIVATE_KEY"`).
- Prefer explicit flags over prompts (`--path`, `--url`, `--args`, `--skip-confirm` where supported).
- Never print secrets.
- Cleanup safety: always get explicit user approval before any cleanup that stops/removes local nodes, networks, or forks.

## Command Guide

### `pop new contract`

- Use for new ink! contract scaffolding.
- `pop new contract --list` for available contracts.

### `pop new chain`

- Use for new Polkadot SDK chain scaffolding.
- `pop new chain --list` for available chains.

### `pop build --path <dir>`

- Use to build contracts or chains with explicit path input.

### `pop test --path <dir>`

- Use to run tests for contracts or chains with explicit path input.

For the following commands:
### `pop up ink-node ... --detach`
### `pop up fork ... --detach`
### `pop up network <zombie.json> --detach`
### `pop up paseo --detach`
### `pop up paseo --parachain asset-hub --detach`
- You MUST use a persistent terminal session.
- After spawned/running output, you MUST wait 10 seconds before interacting.
- Use the endpoint returned by Pop for follow-up calls. For `pop up network` use `ws_uri` values from generated `zombie.json`.
- Treat endpoint verification as a hard gate before any dependent command.
- Do not trust success banner alone; you MUST run:
  - `pop call chain --url <ws-url> --metadata`
- Continue only if the metadata call succeeds (exit code `0` and pallet list output). If it fails, do not proceed with dependent calls.


### `pop up <contract-dir> --constructor <name> --args <arg>... --suri "$PRIVATE_KEY" --url <ws-url> --skip-confirm`

- `--skip-confirm` requires all constructor args via `--args`.
- `pop up <path>` checks only the provided path; it does not recurse into subdirectories.

### `pop verify --path <contract-dir> --url <ws-url> --address <contract-address> --image <image-tag>`

- For deployed contract verification, first build with `pop build --path . --verifiable`, then deploy using `pop up ... --skip-build`, and verify with the exact same `--image` tag.
- Run verifiable workflows from inside the project with `--path .` (not absolute host paths), because Docker path mapping can break `cargo metadata`.

### `pop call chain --url <ws-url> --pallet <pallet> --function <snake_case_name> --args <arg>... --suri "$PRIVATE_KEY" --skip-confirm`

- First time calling a chain use `pop call chain --metadata` for which pallets, + `--pallet <pallet_name>` for pallet info.
- Evaluate both exit code and output text of return.
- Success requires exit code `0` and exact success output:
     - `Call complete.`
     - for extrinsics, also `Extrinsic Submitted with hash:`
- `--function` use `snake_case` (example: `transfer_keep_alive`).
- For calls expecting `MultiAddress` (example: `Balances.transfer_keep_alive`), pass destination as `Id(0x...)`.

### `pop call contract --path <contract-dir> --contract 0x... --message <name> --args <arg>... --suri "$PRIVATE_KEY" --url <ws-url> --skip-confirm`

- Evaluate both exit code and output text.
- Success requires exit code `0` and exact success output:
    - `Call completed successfully!`
    - `Contract calling complete.`
- Default provide not only if user specifies otherwise `--gas` and `--proof-size` must be provided together; passing only one fails argument validation.

### `pop convert address <value>`

- Use for address format conversion utilities.
- Do not assume strict round-trip string equality across families (`SS58 -> EVM -> SS58` may return a different SS58 representation/prefix).

### `pop clean node`

- Requires explicit user approval (global cleanup safety rule).
- Use targeted cleanup first when PIDs are known: `--pid`.
- Use `--all` only when explicitly requested, since it can terminate unrelated local processes.

### `pop clean network`

- Requires explicit user approval (global cleanup safety rule).
- Prefer targeted cleanup by base dir or `zombie.json`.
- Use `--all` only when explicitly requested.
