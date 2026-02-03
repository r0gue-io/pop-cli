# AGENTS.md — Pop CLI

## RULE 0 - THE FUNDAMENTAL OVERRIDE PREROGATIVE

If I tell you to do something, even if it goes against what follows below, YOU MUST LISTEN TO ME. I AM IN CHARGE, NOT YOU.

## RULE NUMBER 1: NO FILE DELETION

**YOU ARE NEVER ALLOWED TO DELETE A FILE WITHOUT EXPRESS PERMISSION.** Even a new file that you yourself created, such as a test code file. You have a horrible track record of deleting critically important files or otherwise throwing away tons of expensive work. As a result, you have permanently lost any and all rights to determine that a file or folder should be deleted.

**YOU MUST ALWAYS ASK AND RECEIVE CLEAR, WRITTEN PERMISSION BEFORE EVER DELETING A FILE OR FOLDER OF ANY KIND.**

## Irreversible Git & Filesystem Actions — DO NOT EVER BREAK GLASS

1. **Absolutely forbidden commands:** `git reset --hard`, `git clean -fd`, `rm -rf`, or any command that can delete or overwrite code/data must never be run unless the user explicitly provides the exact command and states, in the same message, that they understand and want the irreversible consequences.
2. **No guessing:** If there is any uncertainty about what a command might delete or overwrite, stop immediately and ask the user for specific approval. "I think it's safe" is never acceptable.
3. **Safer alternatives first:** When cleanup or rollbacks are needed, request permission to use non-destructive options (`git status`, `git diff`, `git stash`, copying to backups) before ever considering a destructive command.
4. **Mandatory explicit plan:** Even after explicit user authorization, restate the command verbatim, list exactly what will be affected, and wait for a confirmation that your understanding is correct. Only then may you execute it—if anything remains ambiguous, refuse and escalate.
5. **Document the confirmation:** When running any approved destructive command, record (in the session notes / final response) the exact user text that authorized it, the command actually run, and the execution time. If that record is absent, the operation did not happen.

## Toolchain: Rust & Cargo

We only use **Cargo** in this project, NEVER any other package manager.

1. **Edition:** Rust 2024 (see `rust-toolchain.toml`).
2. **Dependency versions:** Explicit versions for stability.
3. **Configuration:** Cargo.toml only.
4. **Unsafe code:** Forbidden (`#![forbid(unsafe_code)]`).

## Code Editing Discipline

### No Script-Based Changes

**NEVER** run a script that processes/changes code files in this repo. Brittle regex-based transformations create far more problems than they solve.

1. **Always make code changes manually**, even when there are many instances.
2. For many simple changes: use parallel subagents.
3. For subtle/complex changes: do them methodically yourself.

### No File Proliferation

If you want to change something or add a feature, **revise existing code files in place**.

**NEVER** create variations like:

- `mainV2.rs`
- `main_improved.rs`
- `main_enhanced.rs`

New files are reserved for **genuinely new functionality** that makes zero sense to include in any existing file. The bar for creating new files is **incredibly high**.

## Terminology

- Prefer **chain** over **parachain** in docs and user-facing text unless the specific technical term is required.

## Backwards Compatibility

2. Never create wrapper functions for deprecated APIs.
3. Just fix the code directly.

## Compiler Checks (CRITICAL)

**After any substantive code changes, you MUST verify no errors were introduced:**

```bash
# Check for compiler errors and warnings
cargo check --all-targets

# Check for clippy lints (pedantic + nursery are enabled)
cargo clippy --all-targets -- -D warnings

# Verify formatting
cargo +nightly fmt --check
```

If you see errors, **carefully understand and resolve each issue**. Read sufficient context to fix them the RIGHT way.

## Build, Test, and Development Commands

1. `cargo build --no-default-features --features chain` builds only parachain features.
2. `cargo build --no-default-features --features contract` builds only smart-contract features.

## Testing

We use `cargo nextest` for faster test runs.

```bash
cargo install cargo-nextest
```

Run only the specific tests you wrote or changed. Do not run the full suite unless explicitly requested.

Run the unit tests only:

```bash
# Recommended
cargo nextest run --lib --bins
# If you don't have nextest installed
cargo test --lib --bins
```

To run the integration tests relating to Smart Contracts:

```bash
cargo nextest run --no-default-features --features contract --test contract
```

To run the integration tests relating to Parachains:

```bash
cargo nextest run --no-default-features --features chain --test chain
cargo nextest run --no-default-features --features chain --test metadata
```

Run all tests (unit + integration):

```bash
cargo nextest run
```

Running tests may exhaust GitHub REST API rate limits. Use a personal access token via the `GITHUB_TOKEN` environment variable.

## Security/Advisory Checks

We use `cargo-deny` locally to check advisories and licenses.

```bash
cargo install cargo-deny
cargo deny check

# Advisories only
cargo deny check advisories
# Licenses only
cargo deny check licenses
```

## GitHub & PR Guidelines

1. Commit messages follow Conventional Commits (e.g., `feat: ...`, `fix: ...`, `refactor: ...`).
2. PRs include a clear description, reference issues/PR numbers when applicable, and note test commands run.

## Third-Party Library Usage

If you aren't 100% sure how to use a third-party library, **SEARCH ONLINE** to find the latest documentation and current best practices.

## ast-grep vs ripgrep

**Use `ast-grep` when structure matters.** It parses code and matches AST nodes, ignoring comments/strings, and can **safely rewrite** code.

1. Refactors/codemods: rename APIs, change import forms.
2. Policy checks: enforce patterns across a repo.
3. Editor/automation: LSP mode, `--json` output.

**Use `ripgrep` when text is enough.** Fastest way to grep literals/regex.

1. Recon: find strings, TODOs, log lines, config values.
2. Pre-filter: narrow candidate files before ast-grep.

### Rule of Thumb

1. Need correctness or **applying changes** → `ast-grep`.
2. Need raw speed or **hunting text** → `rg`.
3. Often combine: `rg` to shortlist files, then `ast-grep` to match/modify.

### Rust Examples

```bash
# Find structured code (ignores comments)
ast-grep run -l Rust -p 'fn $NAME($$$ARGS) -> $RET { $$$BODY }'

# Find all unwrap() calls
ast-grep run -l Rust -p '$EXPR.unwrap()'

# Quick textual hunt
rg -n 'println!' -t rust

# Combine speed + precision
rg -l -t rust 'unwrap\(' | xargs ast-grep run -l Rust -p '$X.unwrap()' --json
```

## Morph Warp Grep — AI-Powered Code Search

**Use `mcp__morph-mcp__warp_grep` for exploratory "how does X work?" questions.** An AI agent expands your query, greps the codebase, reads relevant files, and returns precise line ranges with full context.

**Use `ripgrep` for targeted searches.** When you know exactly what you're looking for.

**Use `ast-grep` for structural patterns.** When you need AST precision for matching/rewriting.

### When to Use What

| Scenario | Tool | Why |
|----------|------|-----|
| "How is streaming implemented?" | `warp_grep` | Exploratory; don't know where to start |
| "Where is the Anthropic provider?" | `warp_grep` | Need to understand architecture |
| "Find all uses of `serde_json::from_str`" | `ripgrep` | Targeted literal search |
| "Find files with `println!`" | `ripgrep` | Simple pattern |
| "Replace all `unwrap()` with `expect()`" | `ast-grep` | Structural refactor |

### warp_grep Usage

```
mcp__morph-mcp__warp_grep(
  repoPath: "/path/to/repo",
  query: "How does the SSE parser handle streaming events?"
)
```

### Anti-Patterns

1. **Don't** use `warp_grep` to find a specific function name → use `ripgrep`.
2. **Don't** use `ripgrep` to understand "how does X work" → wastes time with manual reads.
3. **Don't** use `ripgrep` for codemods → risks collateral edits.

## UBS — Ultimate Bug Scanner

**Golden Rule:** `ubs <changed-files>` before every commit. Exit 0 = safe. Exit >0 = fix & re-run.

### Commands

```bash
ubs file.rs file2.rs                    # Specific files (< 1s) — USE THIS
ubs $(git diff --name-only --cached)    # Staged files — before commit
ubs --only=rust,toml src/               # Language filter (3-5x faster)
ubs --ci --fail-on-warning .            # CI mode — before PR
ubs .                                   # Whole project (ignores target/, Cargo.lock)
```

### Output Format

```
⚠️  Category (N errors)
    file.rs:42:5 – Issue description
    💡 Suggested fix
Exit code: 1
```

Parse: `file:line:col` → location | 💡 → how to fix | Exit 0/1 → pass/fail.

### Fix Workflow

1. Read finding → category + fix suggestion.
2. Navigate `file:line:col` → view context.
3. Verify real issue (not false positive).
4. Fix root cause (not symptom).
5. Re-run `ubs <file>` → exit 0.
6. Commit.

### Bug Severity

1. **Critical (always fix):** Memory safety, use-after-free, data races, SQL injection.
2. **Important (production):** Unwrap panics, resource leaks, overflow checks.
3. **Contextual (judgment):** TODO/FIXME, println! debugging.

## Note on Built-in TODO Functionality

If I ask you to explicitly use your built-in TODO functionality, don't complain about this and say you need to use beads. You can use built-in TODOs if I tell you specifically to do so. Always comply with such orders.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up.
2. **Run quality gates** (if code changed) - Tests, linters, builds.
3. **Update issue status** - Close finished work, update in-progress items.
4. **PUSH TO REMOTE** - This is MANDATORY:

```bash
git pull --rebase
git add <other files>   # Stage code changes
git commit -m "..."     # Commit everything
git push
git status  # MUST show "up to date with origin"
```

5. **Clean up** - Clear stashes, prune remote branches.
6. **Verify** - All changes committed AND pushed.
7. **Hand off** - Provide context for next session.

**CRITICAL RULES:**

1. Work is NOT complete until `git push` succeeds.
2. NEVER stop before pushing - that leaves work stranded locally.
3. NEVER say "ready to push when you are" - YOU must push.
4. If push fails, resolve and retry until it succeeds.
