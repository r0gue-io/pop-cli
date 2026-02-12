# Pop CLI Skill

## Scope
Use `pop` (Pop CLI) to scaffold, build, test, launch, deploy, and call for Polkadot SDK chains and ink! contracts.

## Rules
- Use plain `pop` commands only. One command per step.
- Do not chain commands or use shell wrappers/operators (`&&`, `;`, `|`, command substitution, scripts/functions).
- Exception: env expansion is allowed only for signer secret on `--suri` (example: `--suri "$PRIVATE_KEY"`).
- Prefer non-interactive flags where supported (`-y`, `--skip-confirm`).
- Never print secrets.

## Cleanup
Always clean up after flows:
- `pop clean node --all` or `pop clean node --pid <PID>...`
- `pop clean network --all` or `pop clean network <base-dir-or-zombie.json>`

## Tested Gotchas
- `pop new contract` does not accept `-y`.
- `pop new chain` does not accept `-y`.
- `pop up <contract-dir> --skip-confirm` requires all constructor args via `--args`.
- `pop call contract` cannot combine `--dev` with `--skip-confirm`.
- `pop call chain --function` expects runtime/source naming, often `snake_case` (example: `transfer_keep_alive`).
- For calls expecting `MultiAddress` (example: `Balances.transfer_keep_alive`), destination should be passed as `Id(0x...)`.
- `pop up <contract-dir>` may own local node lifecycle and terminate it; for multi-step flows, use explicit non-default node ports and explicit `--url`.
- `pop up network --detach` may report running while endpoints are not reachable; verify with immediate read call.
- `pop up network --cmd` runs one command by whitespace splitting (`split_whitespace`), not a shell.
- For call flows, evaluate both exit code and output markers, not exit code alone.
- Capture both stdout and stderr when parsing outputs.

## Common Flows
- Contracts: `pop new contract`, `pop build --path ...`, `pop test --path ...`, `pop up ink-node`, `pop up <contract>`, `pop call contract ...`
- Chains: `pop new chain`, `pop build --path ...`, `pop test --path ...`, `pop up network ...`, `pop call chain ...`
- Utilities: `pop convert address`, `pop clean node`, `pop clean network`
