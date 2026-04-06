# Sandbox

Tests looper against real GitHub projects to measure detection coverage, catch regressions, and identify missing patterns.

## Prerequisites

- **jq** -- `brew install jq`
- **git**
- **looper** binary -- `cargo build --release` from the project root

## Quick Start

```bash
./bench.sh setup              # clone all test projects
./bench.sh scan               # run looper against each
./bench.sh approve express    # review results, then approve as baseline
./bench.sh compare            # after detector changes, check for regressions
./bench.sh report             # generate aggregate summary
```

## Commands

| Command | Description |
|---------|-------------|
| `setup [project]` | Clone/update repos from `projects.json` |
| `scan [project]` | Run looper, save JSON to `results/` |
| `compare [project]` | Diff results vs approved baselines, show gaps |
| `approve <project>` | Promote current results to approved baseline |
| `report` | Generate `reports/summary.md` with aggregate stats |
| `add <url> [name] [ref]` | Add a new project to the registry |
| `clean` | Remove `repos/`, `results/`, `reports/` |

## Directory Layout

```
sandbox/
  projects.json          # project registry (committed)
  bench.sh               # orchestration script (committed)
  expected/              # ground truth (committed)
    <project>/
      approved.json      # confirmed correct findings
      known-gaps.json    # patterns looper can't detect yet
  repos/                 # cloned repos (gitignored)
  results/               # latest scan output (gitignored)
  reports/               # generated reports (gitignored)
```

## Workflow

### Adding a new project

1. `./bench.sh add https://github.com/org/repo my-project v1.0.0`
2. `./bench.sh setup my-project`
3. `./bench.sh scan my-project`
4. Review `results/my-project.json` -- remove any false positives manually
5. `./bench.sh approve my-project`
6. Inspect the source code for patterns looper missed
7. Document gaps in `expected/my-project/known-gaps.json`

### After detector improvements

1. `./bench.sh scan` (rescans all projects)
2. `./bench.sh compare` (shows regressions, new findings, resolved gaps)
3. `./bench.sh report` (generates `reports/summary.md`)
4. Approve new true positives, remove resolved gaps, commit

### Understanding the report

- **Matched** -- same finding in both approved and actual (stable)
- **Changed** -- same id but confidence/locations differ (review)
- **New findings** -- detected but not yet approved (true positive or false positive?)
- **Regressions** -- was approved, now missing (bug)
- **Now detected** -- a known gap that looper now catches (win!)
- **Coverage** = approved / (approved + remaining gaps)

## `known-gaps.json` Format

```json
{
  "project": "example",
  "gaps": [
    {
      "id": "env_var::SECRET_KEY",
      "category": "env_var",
      "name": "SECRET_KEY",
      "reason": "accessed via custom config wrapper",
      "source_location": "src/config.ts:15",
      "blocked_by": "config-library",
      "priority": "high"
    }
  ]
}
```

Standard `blocked_by` values:

| Value | Meaning |
|-------|---------|
| `data-flow-tracking` | Requires following variable assignments |
| `framework-specific` | Framework API not yet recognized |
| `dynamic-value` | Value computed at runtime |
| `relative-url` | Relative URL path (no host) |
| `config-library` | Config library not yet supported |
| `template-literal` | Dynamic template string |
