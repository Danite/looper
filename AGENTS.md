# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd prime` for full workflow context.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work atomically
bd close <id>         # Complete work
bd dolt push          # Push beads data to remote
```

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r source dest
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Clean up** - Clear stashes, prune remote branches
5. **Hand off** - Provide context for next session, and suggest a commit message

**CRITICAL RULES:**
- Don't commit or push the changes I will do that myself
- Double check that our code is correct and follow the best practices
- Don't include self explanatory comments, only if it's really necessary
<!-- END BEADS INTEGRATION -->

## Quality Gates

Run before finishing work:

```bash
cargo build          # must compile clean
cargo test           # all tests pass
cargo clippy         # no warnings
cargo fmt -- --check # formatting check
```

## Project Conventions

### Architecture
- Single crate: binary (`main.rs`, `cli.rs`, `commands/`, `scan.rs`) + library (everything else)
- Pipeline: walk -> parse -> resolve -> detect -> merge -> output
- Detectors: `Detector` trait + registry pattern; each is a ZST struct + private `*Collector` using `oxc_ast_visit::Visit`
- Output formatters: free functions per format, dispatched by string match in `format_and_write`

### Coding Standards
- No `anyhow`/`thiserror` -- errors are `String` or `i32` exit codes; commands (`cmd_*`) use `eprintln!` + `colored`
- Exit codes: 0 = success, 1 = findings/errors, 2 = fatal (no files, no infra)

### Testing
- Integration tests: `tests/*_test.rs` (one per area)
- Fixtures: `tests/fixtures/` (TypeScript samples + infra files)
- Snapshots: `insta` (`tests/snapshots/`); update with `cargo insta review`

# Project Spec

Development specs and internal docs are in the /working/ directory