# looper

**Static analysis tool that discovers implicit "shadow dependencies" in JavaScript/TypeScript codebases.**

Your code depends on more than what's in `package.json`. It reads env vars, assumes files exist on disk, reaches out to network hosts, shells out to system binaries, and binds to ports -- none of which are tracked by any package manager. `looper` finds all of them.

## Problem

A project's `package.json` declares explicit dependencies. But code also depends on:

- `DATABASE_URL` being set in the environment
- `ffmpeg` being installed on the host
- `https://api.stripe.com` being reachable
- `./config/settings.json` existing on disk
- Port `5432` being available

These **shadow dependencies** cause "works on my machine" failures, broken CI, mysterious production crashes, and painful onboarding.

## Quick Start

```bash
cargo install looper

# Scan current directory
looper .

# Generate a JSON manifest
looper . --format json -o shadow-deps.json

# Initialize config for your project
looper init
```

## What It Detects

| Category | Example | Pattern |
|----------|---------|---------|
| **Env vars** | `process.env.DATABASE_URL` | `process.env`, `import.meta.env`, `Deno.env.get`, `Bun.env` |
| **Filesystem** | `fs.readFileSync("./config.json")` | `fs.*`, `path.join`, `Deno.readTextFile`, `Bun.file` |
| **Network** | `fetch("https://api.stripe.com")` | `fetch`, `axios`, `http.request`, `new URL`, `WebSocket` |
| **Binaries** | `exec("ffmpeg -i input.mp4")` | `exec`, `spawn`, `execa`, zx `$` templates |
| **Ports** | `app.listen(3000)` | `.listen()`, `net.createConnection`, connection strings |
| **Platform** | `process.platform === "darwin"` | Unix paths, Windows commands, `os.*`, `path.sep` |

## Output Formats

```bash
# Colored terminal output (default)
looper .

# JSON manifest for machines
looper . --format json -o shadow-deps.json

# SARIF for GitHub Code Scanning
looper . --format sarif -o report.sarif

# Interactive HTML report
looper . --format html -o report.html
```

## CLI Reference

```
looper [OPTIONS] [PATHS]...          Scan (default command)
looper check [OPTIONS]               Compare scan against manifest
looper init                          Generate config and manifest
looper diff <OLD> <NEW>              Compare two manifests
looper drift [OPTIONS]               Compare code needs vs infrastructure
looper doctor [OPTIONS]              Runtime health check
```

### Scan options

```
-f, --format <FMT>         terminal | json | sarif | html
-o, --output <FILE>        Write to file instead of stdout
--only <CATEGORIES>         Filter: env,fs,network,shell,ports,platform
--exclude <CATEGORIES>      Exclude categories
--min-confidence <LEVEL>    Filter: low | medium | high
--baseline <FILE>           Suppress findings in baseline manifest
--ignore <PATTERNS>         Additional glob patterns to ignore
--no-gitignore              Don't respect .gitignore
-q, --quiet                 One-line summary
-v, --verbose               Show per-file progress
-j, --threads <N>           Parallel threads (default: auto)
--config <FILE>             Path to config file
```

### Check options

```
-m, --manifest <FILE>       Manifest path (default: shadow-deps.json)
--fail-on-new               Exit 1 on new deps (default: true)
--fail-on-removed           Exit 1 on removed deps
--update                    Update manifest with current scan
```

## CI Integration

### Step 1: Generate initial manifest

```bash
looper . --format json -o shadow-deps.json
git add shadow-deps.json
git commit -m "chore: add shadow dependency manifest"
```

### Step 2: Add CI check

```yaml
# .github/workflows/shadow-deps.yml
name: Shadow Deps Check
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install looper
        run: cargo install looper
      - name: Check for undeclared shadow deps
        run: looper check --manifest shadow-deps.json
      - name: Check infrastructure drift
        run: looper drift --fail-on-missing
```

When a PR introduces `process.env.NEW_KEY` without updating the manifest:

```
ERROR: 1 undeclared shadow dependency found

  + env_var: NEW_KEY  (src/feature.ts:42)

Update the manifest:  looper check --update
```

## Configuration

`looper init` creates `looper.config.json`:

```json
{
  "include": ["src/**", "lib/**"],
  "exclude": [
    "**/*.test.ts",
    "**/*.spec.ts",
    "**/node_modules/**",
    "**/dist/**"
  ],
  "categories": {
    "env_var": { "ignore": ["NODE_ENV", "CI"] },
    "network": { "ignore": ["localhost", "127.0.0.1"] }
  },
  "min_confidence": "low",
  "manifest": "shadow-deps.json"
}
```

## Incremental Adoption

For large codebases, use `--baseline` to suppress existing findings and only flag new ones:

```bash
# Generate baseline from current state
looper . --format json -o baseline.json

# In CI, only fail on NEW shadow deps
looper . --baseline baseline.json
```

## Comparing Manifests

```bash
# See what changed between two versions
looper diff old-manifest.json new-manifest.json

# Machine-readable diff
looper diff old.json new.json --format json
```

## Infrastructure Drift

Compare what your code needs against what your docker-compose, .env, and Dockerfile provide:

```bash
# Auto-detect infra files in current directory
looper drift

# Explicit infra files
looper drift --compose docker-compose.yml --env .env --dockerfile Dockerfile

# CI gate: fail if code needs something infra doesn't provide
looper drift --fail-on-missing
```

Output shows missing, external, unused, and covered deps:

```
MISSING FROM INFRASTRUCTURE (4):
  env_var   STRIPE_SECRET_KEY      not in .env or docker-compose
  binary    ffmpeg                 not in Dockerfile

EXTERNAL (2):
  network   api.stripe.com         external API

COVERED (5):
  env_var   DATABASE_URL           .env line 3
  port      :6379                  docker-compose service redis
```

## Runtime Health Check

Verify your development environment is fully set up:

```bash
# Check all shadow deps are satisfied
looper doctor

# Use pre-scanned manifest (faster)
looper doctor --manifest shadow-deps.json

# CI gate: fail if anything is missing
looper doctor --fail-on-error
```

Output:

```
looper doctor — runtime health check

 ENV VARS
  ✓ DATABASE_URL         set
  ✗ STRIPE_SECRET_KEY    missing — export STRIPE_SECRET_KEY=<value>
  ✓ PORT                 optional (default: 3000)

 BINARIES
  ✓ git                  /usr/bin/git
  ✗ ffmpeg               not found — install: apt-get install ffmpeg

 Summary: 3 passed, 2 failed
```

The killer onboarding workflow: clone repo, run `looper doctor`, fix everything red.

## Exit Codes

| Command | 0 | 1 | 2 |
|---------|---|---|---|
| `scan` | Success | Parse errors present | No files found |
| `check` | Clean | Undeclared deps found | Manifest not found |
| `diff` | Always | -- | -- |
| `init` | Success | Write error | -- |
| `drift` | No missing (or flag not set) | Missing items (with `--fail-on-missing`) | No infra files |
| `doctor` | All pass (or flag not set) | Checks failed (with `--fail-on-error`) | No deps found |

## Performance

- Parallel file scanning via rayon
- oxc parser (3x faster than swc)
- 6,889 files scanned in ~1.5 seconds

## License

MIT
