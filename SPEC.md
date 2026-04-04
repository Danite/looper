# looper

**Static analysis tool that discovers implicit "shadow dependencies" in JavaScript/TypeScript codebases.**

Your code depends on more than what's in `package.json`. It reads env vars, assumes files exist on disk, reaches out to network hosts, shells out to system binaries, and binds to ports — none of which are tracked by any package manager. `looper` finds all of them.

---

## Problem

A project's `package.json` declares explicit dependencies. But code also depends on:

- `DATABASE_URL` being set in the environment
- `ffmpeg` being installed on the host
- `https://api.stripe.com` being reachable
- `./config/settings.json` existing on disk
- Port `5432` being available

These **shadow dependencies** are invisible. They cause "works on my machine" failures, broken CI, mysterious production crashes, and painful onboarding. No tool maps them.

---

## Solution

A Rust CLI that statically analyzes JS/TS source files, walks the AST, and extracts every implicit dependency into a structured manifest. Outputs JSON for machines, SARIF for CI systems, a colored terminal report for humans, and a self-contained HTML page for visual exploration.

### Supported File Types

**Phase 1:** `.js`, `.jsx`, `.ts`, `.tsx`, `.mjs`, `.cjs`, `.mts`, `.cts`

**Future:** `.vue`, `.svelte`, `.astro` (extract `<script>` blocks before parsing)

---

## Detection Categories

### 1. Environment Variables

Patterns detected:

```js
process.env.DATABASE_URL                    // direct access
process.env["STRIPE_KEY"]                   // bracket access
const { NODE_ENV, PORT } = process.env      // destructuring
const dbUrl = process.env?.DB_URL           // optional chaining
import.meta.env.VITE_API_URL               // Vite / modern bundlers
import.meta.env.PUBLIC_API_KEY             // Astro
import.meta.env.MODE                       // universal
```

Runtime-specific patterns:

```js
Deno.env.get("DATABASE_URL")               // Deno
Bun.env.API_KEY                            // Bun
```

Library-aware patterns:

```js
require("dotenv").config()                  // dotenv usage flagging
envalid.cleanEnv(process.env, { ... })      // envalid schema extraction
```

Default/fallback tracking:

```js
process.env.PORT || 3000                    // → name: PORT, default: "3000", required: false
process.env.API_KEY ?? "sk-default"         // → name: API_KEY, default: "sk-default", required: false
process.env.DATABASE_URL                    // → name: DATABASE_URL, required: true (no fallback)
```

### 2. Filesystem Access

Patterns detected:

```js
fs.readFileSync("./config.json")            // sync read
fs.promises.readFile("/etc/ssl/certs/ca.pem") // async read
fs.existsSync(path.join(__dirname, ".."))   // existence check
fs.writeFileSync("/tmp/cache.json", data)   // write
fs.mkdirSync("./uploads")                   // directory creation
```

Also catches:

```js
path.resolve("./data")                      // path.resolve with string literal
path.join(__dirname, "config.json")         // path.join with string literal
`${__dirname}/config.json`                  // template literal path construction
require(`./locales/${lang}.json`)           // partial fs dep (directory known, file dynamic)
await import(`./plugins/${name}.js`)        // dynamic import with path root
Deno.readTextFile("./config.json")          // Deno fs
Bun.file("./data.json")                     // Bun fs
```

### 3. Network Endpoints

Patterns detected:

```js
fetch("https://api.stripe.com/v1/charges")  // fetch API
axios.get("https://auth.service.internal")  // axios
http.request({ hostname: "db.example.com" })// http module
new URL("wss://realtime.example.com")       // URL construction
new WebSocket("ws://localhost:8080")        // WebSocket
```

JSX attribute values:

```jsx
<img src={`${process.env.CDN_URL}/logo.png`} />  // env var in JSX
<Script src="https://analytics.example.com" />    // URL in JSX attribute
```

Extracts: host, port, protocol, path pattern.

### 4. System Binaries (Shell Commands)

Patterns detected:

```js
exec("ffmpeg -i input.mp4 output.webm")     // child_process.exec
execSync("git rev-parse HEAD")               // sync variant
spawn("convert", ["-resize", "100x100"])     // child_process.spawn
execa("docker", ["build", "."])              // execa library
$`grep -r "TODO" src/`                       // zx tagged template
```

Extracts: binary name, arguments pattern.

### 5. Ports

Patterns detected:

```js
app.listen(3000)                             // express/http listen
server.listen(process.env.PORT || 8080)      // with fallback
net.createConnection({ port: 6379 })         // outbound connection
new Redis("redis://localhost:6379")          // connection string
mongoose.connect("mongodb://localhost:27017")// mongo connection
```

Extracts: port number, direction (listen vs connect), protocol hint.

Connection strings are cross-category extraction targets. A URI like `postgres://user@db.internal:5432/mydb` emits findings in **network** (host: `db.internal`), **ports** (`:5432`, connect), and potentially **env_var** if the string originates from `process.env.DATABASE_URL`.

```js
Bun.serve({ port: 3000 })                  // Bun server
```

### 6. Platform Assumptions

Patterns detected:

```js
fs.openSync("/dev/null")                     // Unix-only path
child_process.exec("cmd /c dir")             // Windows-only
path.sep === "/"                             // separator assumption
process.platform === "darwin"                // platform guard
os.homedir()                                 // implicit ~ dependency
```

Extracts: assumed platform, guarded vs unguarded usage.

---

## Confidence Levels

Not all detections are equal:

| Level | Meaning | Example |
|-------|---------|---------|
| **High** | Literal string, unambiguous | `process.env.API_KEY` |
| **Medium** | Inferable from context | `env[config.keyName]` where `keyName = "API_KEY"` nearby |
| **Low** | Dynamic, can only flag the pattern | `process.env[userInput]` |

The manifest includes confidence per finding so consumers can filter by reliability.

---

## Import Binding Resolution

The filesystem, shell, network, and port detectors need to know what local variable names refer to. When code says `fs.readFileSync(...)`, the tool must verify that `fs` is bound to the `'fs'` module, not some unrelated variable.

A **binding resolution pass** runs before detectors and builds a map of `local_name -> (module_source, imported_name)`:

```js
import { readFileSync } from 'fs'           // readFileSync -> ("fs", "readFileSync")
const fs = require('fs')                     // fs -> ("fs", "*")
import * as cp from 'node:child_process'     // cp -> ("child_process", "*")
const { readFile } = require('fs/promises')  // readFile -> ("fs/promises", "readFile")
```

This map is passed to each detector so they match calls against known Node built-in modules rather than arbitrary names. This covers 95%+ of real code without needing full scope analysis.

For deeper resolution (re-exports, cross-file aliasing), `oxc_semantic` can be added in a later phase.

---

## Error Handling

The tool must be resilient. A single broken file should never crash a scan of hundreds of files.

| Situation | Behavior |
|-----------|----------|
| Parse error in a file | Collect the error, skip the file, continue scanning. Report in `--verbose` mode. |
| Permission denied | Skip the file with a warning. |
| Symlinks | Follow symlinks with cycle detection (the `ignore` crate supports this). |
| Binary / non-UTF-8 files | Skip silently even if the extension matches. |
| Empty files | Skip silently. |
| No files found | Exit with code 2 and a helpful message. |

Exit codes for `scan`: 0 = success, 1 = scan completed with parse errors, 2 = no files found.

Exit codes for `check`: 0 = clean, 1 = undeclared deps found, 2 = manifest file not found.

---

## Output Formats

### Terminal (default)

```
looper v0.1.0 — scanned 142 files in 48ms

 ENV VARS (7 found)
 ──────────────────────────────────────────────────
  DATABASE_URL        src/db.ts:12         high
  STRIPE_SECRET_KEY   src/billing.ts:8     high
  NODE_ENV            src/config.ts:3      high
  PORT                src/server.ts:5      high
  REDIS_URL           src/cache.ts:2       high
  API_TIMEOUT         src/http.ts:14       medium
  LOG_LEVEL           src/logger.ts:1      high

 NETWORK (3 found)
 ──────────────────────────────────────────────────
  api.stripe.com      src/billing.ts:22    high
  sentry.io           src/errors.ts:5      high
  localhost:6379      src/cache.ts:8       high

 BINARIES (1 found)
 ──────────────────────────────────────────────────
  ffmpeg              src/video.ts:15      high

 FILESYSTEM (2 found)
 ──────────────────────────────────────────────────
  ./config/app.json   src/config.ts:8      high
  /tmp/cache/         src/cache.ts:30      high

 PORTS (2 found)
 ──────────────────────────────────────────────────
  :3000 (listen)      src/server.ts:45     high
  :5432 (connect)     src/db.ts:3          high

 Summary: 15 shadow deps across 6 categories
```

### JSON Manifest (`--format json`)

```json
{
  "schema_version": "1",
  "tool_version": "0.1.0",
  "scanned_at": "2026-04-04T12:00:00Z",
  "root": ".",
  "files_scanned": 142,
  "scan_duration_ms": 48,
  "deps": [
    {
      "id": "env_var::DATABASE_URL",
      "category": "env_var",
      "name": "DATABASE_URL",
      "confidence": "high",
      "metadata": {
        "required": true,
        "source_pattern": "direct_access"
      },
      "locations": [
        { "file": "src/db.ts", "line": 12, "col": 5 }
      ]
    },
    {
      "id": "env_var::PORT",
      "category": "env_var",
      "name": "PORT",
      "confidence": "high",
      "metadata": {
        "required": false,
        "default_value": "3000",
        "source_pattern": "direct_access"
      },
      "locations": [
        { "file": "src/server.ts", "line": 5, "col": 3 }
      ]
    },
    {
      "id": "network::api.stripe.com",
      "category": "network",
      "name": "api.stripe.com",
      "confidence": "high",
      "metadata": {
        "protocol": "https",
        "path_pattern": "/v1/charges"
      },
      "locations": [
        { "file": "src/billing.ts", "line": 22, "col": 3 }
      ]
    },
    {
      "id": "binary::ffmpeg",
      "category": "binary",
      "name": "ffmpeg",
      "confidence": "high",
      "metadata": {
        "args_pattern": "-i * *"
      },
      "locations": [
        { "file": "src/video.ts", "line": 15, "col": 5 }
      ]
    },
    {
      "id": "filesystem::./config/app.json",
      "category": "filesystem",
      "name": "./config/app.json",
      "confidence": "high",
      "metadata": {
        "operation": "read",
        "is_absolute": false
      },
      "locations": [
        { "file": "src/config.ts", "line": 8, "col": 5 }
      ]
    },
    {
      "id": "port::3000::listen",
      "category": "port",
      "name": ":3000",
      "confidence": "high",
      "metadata": {
        "direction": "listen",
        "protocol_hint": "http"
      },
      "locations": [
        { "file": "src/server.ts", "line": 45, "col": 1 }
      ]
    }
  ]
}
```

Every dependency carries an `id` (deterministic `category::name` key for stable diffing), a `confidence` level, and a `metadata` object. The `schema_version` is separate from `tool_version` so the manifest format can evolve independently.

### SARIF (`--format sarif`)

SARIF (Static Analysis Results Interchange Format) is the industry standard for static analysis output. Producing SARIF means:

- GitHub Code Scanning renders findings as inline PR annotations for free
- VS Code's SARIF Viewer extension shows them in the Problems panel
- Compatible with any CI system that consumes SARIF (Azure DevOps, GitLab, etc.)

Each shadow dep maps to a SARIF `result` with the category as the `ruleId`, the finding as the `message`, and file/line/col as the `physicalLocation`.

### HTML Report (`--format html`)

A self-contained HTML file (no external dependencies) with:

- Interactive graph: files as nodes, shadow deps as colored edges grouped by category
- Filterable table: sort/filter by category, confidence, file
- Summary stats: total deps, breakdown by category, confidence distribution
- Click-to-copy: file paths and dep names for quick reference

---

## CLI Interface

```
USAGE:
    looper [OPTIONS] [PATHS]...
    looper <COMMAND> [OPTIONS]

COMMANDS:
    scan (default)    Scan for shadow dependencies
    check             Compare scan against declared manifest
    init              Generate config file and initial manifest
    diff              Compare two manifest files

SCAN OPTIONS:
    -f, --format <FMT>     Output format: terminal (default), json, sarif, html
    -o, --output <FILE>    Write output to file instead of stdout
    --only <CATEGORIES>    Only scan specific categories (comma-separated)
                           Values: env, fs, network, shell, ports, platform
    --exclude <CATEGORIES> Exclude specific categories (comma-separated)
    --min-confidence <LVL> Minimum confidence: low, medium, high
    --baseline <FILE>      Only show findings not in the baseline manifest
    --ignore <PATTERNS>    Additional glob patterns to ignore
    --no-gitignore         Don't respect .gitignore
    -q, --quiet            Only output summary line

CHECK OPTIONS:
    -m, --manifest <FILE>  Path to declared manifest (default: shadow-deps.json)
    --fail-on-new          Exit 1 if new undeclared deps found (default)
    --fail-on-removed      Also exit 1 if declared deps are no longer found
    --update               Update the manifest file with current scan results

DIFF OPTIONS:
    <OLD>                  Path to old manifest
    <NEW>                  Path to new manifest
    -f, --format <FMT>     Output format: terminal (default), json

GLOBAL OPTIONS:
    --config <FILE>        Path to config file (default: auto-detect)
    -j, --threads <N>      Number of parallel threads (default: auto)
    -v, --verbose           Show per-file scanning progress
    --version               Print version
    -h, --help              Print help
```

### Config File

`looper init` generates a `looper.config.json` in the project root. The tool auto-detects this file when run without `--config`.

```json
{
  "include": ["src/**", "lib/**"],
  "exclude": ["**/*.test.ts", "**/*.spec.ts"],
  "categories": {
    "env_var": {
      "ignore": ["NODE_ENV", "CI"]
    },
    "network": {
      "ignore_hosts": ["localhost", "127.0.0.1"]
    }
  },
  "min_confidence": "medium",
  "manifest": "shadow-deps.json"
}
```

---

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
name: Looper Deps Check
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
```

When a PR introduces `process.env.NEW_KEY` without updating the manifest:

```
ERROR: 1 undeclared shadow dependency found

  + env_var: NEW_KEY  (src/feature.ts:42)

Update the manifest:  looper check --update
```

---

## Project Structure

```
looper/
├── Cargo.toml
├── SPEC.md                  ← this file
├── README.md
├── looper.config.json       # Example config file
├── src/
│   ├── main.rs              # CLI entry (clap)
│   ├── lib.rs               # Public API re-exports
│   ├── config.rs            # Config file loading (looper.config.json)
│   ├── walker.rs            # File discovery, .gitignore respect, parallel walk
│   ├── parser.rs            # oxc AST parsing, error recovery, detector dispatch
│   ├── resolver.rs          # Import/require binding resolution
│   ├── types.rs             # ShadowDep, Category, Confidence, Location, Metadata
│   ├── detectors/
│   │   ├── mod.rs           # Detector trait, registry
│   │   ├── env_vars.rs      # Environment variable detection
│   │   ├── filesystem.rs    # Filesystem path detection
│   │   ├── network.rs       # Network endpoint detection
│   │   ├── shell.rs         # System binary detection
│   │   ├── ports.rs         # Port listen/connect detection
│   │   └── platform.rs      # Platform assumption detection
│   ├── output/
│   │   ├── mod.rs           # Output trait
│   │   ├── terminal.rs      # Colored terminal tables
│   │   ├── json.rs          # JSON manifest serialization
│   │   ├── sarif.rs         # SARIF output for CI systems
│   │   └── html.rs          # Self-contained HTML report
│   ├── check.rs             # CI check: diff scan vs manifest
│   └── diff.rs              # Diff two manifest files
├── templates/
│   └── report.html          # HTML report template
└── tests/
    ├── fixtures/            # Sample JS/TS files with known shadow deps
    │   ├── env_vars.ts
    │   ├── filesystem.ts
    │   ├── network.ts
    │   ├── shell.ts
    │   ├── ports.ts
    │   ├── platform.ts
    │   ├── mixed.ts         # Multi-category file for cross-detection tests
    │   └── broken.ts        # File with parse errors for resilience testing
    ├── integration.rs       # End-to-end scan tests
    └── snapshots/           # insta snapshot files for output format tests
```

---

## Rust Dependencies

oxc crates are pre-1.0 and release frequently. Pin via `Cargo.lock`; the versions below are illustrative.

```toml
[dependencies]
oxc_parser = "0.123"        # JS/TS parsing (use latest)
oxc_ast = "0.123"           # AST types
oxc_ast_visit = "0.123"     # Visit trait for AST traversal
oxc_span = "0.123"          # Source spans
oxc_allocator = "0.123"     # Arena allocator for oxc
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }  # ISO 8601 timestamps
ignore = "0.4"              # .gitignore-aware file walking (includes WalkParallel)
colored = "2"               # Terminal colors
rayon = "1.10"              # Parallel file scanning
url = "2"                   # URL parsing for network detector
regex = "1"                 # Pattern matching for connection strings

[dev-dependencies]
insta = { version = "1", features = ["json", "redactions"] }  # Snapshot testing
```

Future additions as needed:

```toml
oxc_semantic = "0.123"      # Scope analysis for deeper binding resolution (Phase 2+)
```

---

## Phased Build Plan

### Phase 1 — Foundation (scaffold + core pipeline)
- Cargo project scaffold with all dependencies
- File walker (`walker.rs`) — discover JS/TS files, respect `.gitignore`, **parallel from day one** via `ignore::WalkParallel` + rayon
- Parser wrapper (`parser.rs`) — parse files with oxc, error recovery (skip broken files, collect errors)
- **Binding resolver** (`resolver.rs`) — track import/require statements, build `local_name -> module_source` map
- Core types (`types.rs`) — `ShadowDep`, `Category`, `Confidence`, `Location`, `Metadata`
- Detector trait (`detectors/mod.rs`) — trait definition, registry pattern
- Env vars detector (`detectors/env_vars.rs`) — the first and most valuable detector (doesn't need binding resolution, validates the full pipeline)
- Basic terminal output — print findings to stdout

**Milestone:** `looper .` scans a project in parallel and prints env var findings.

### Phase 2 — All Detectors
- Filesystem detector (`detectors/filesystem.rs`) — uses binding resolver to match `fs` / `path` module calls
- Network detector (`detectors/network.rs`) — URL literals, `fetch`/`axios`/`http` module calls, JSX attributes
- Shell/binary detector (`detectors/shell.rs`) — uses binding resolver to match `child_process` / `execa` calls
- Ports detector (`detectors/ports.rs`) — `listen`/`connect` calls, connection string parsing (cross-category)
- Platform detector (`detectors/platform.rs`) — platform-specific paths, `process.platform` checks
- Unit tests per detector with inline `#[cfg(test)]` modules

**Milestone:** All six categories detected and printed.

### Phase 3 — Output Formats + Config
- JSON manifest output (`output/json.rs`) — schema v1 with `id`, `metadata`, `schema_version`
- Polished terminal output (`output/terminal.rs`) — grouped tables, colors, summary
- SARIF output (`output/sarif.rs`) — for GitHub Code Scanning and CI
- HTML report (`output/html.rs`) — self-contained file with interactive graph
- Config file loading (`config.rs`) — auto-detect `looper.config.json`
- `init` subcommand — generate config and initial manifest
- CLI flags for `--format`, `--output`, `--only`, `--exclude`, `--min-confidence`, `--baseline`
- Snapshot tests for all output formats via `insta`

**Milestone:** `looper . --format json -o shadow-deps.json` produces a valid manifest. `looper init` bootstraps a project.

### Phase 4 — CI Mode + Polish
- `check` subcommand (`check.rs`) — diff scan against manifest
- `diff` subcommand (`diff.rs`) — compare two manifest files
- `--update` flag to auto-update manifest
- Exit codes: scan (0/1/2), check (0/1/2)
- Integration tests with fixture files (including `mixed.ts` and `broken.ts`)
- README with install instructions, usage examples, CI config examples

**Milestone:** Full CI-ready tool. `looper check` gates PRs. `looper diff` enables manifest review.

---

## Expansion Roadmap (post-v1)

These items should influence architectural decisions now but are implemented after the core tool ships.

### Multi-Language Support
Python (`os.environ`, `subprocess.run`, `open()`), Go (`os.Getenv`, `exec.Command`, `http.ListenAndServe`), Ruby (`ENV[]`, `system()`, `File.read`). The detection patterns are nearly identical across languages; only the parser differs. The `Detector` trait should output a common `ShadowDep` type with language-specific AST traversal encapsulated per-language:

```
Detector (language-agnostic output: Vec<ShadowDep>)
  └── JsDetector (oxc AST)
  └── PyDetector (tree-sitter AST, future)
  └── GoDetector (tree-sitter AST, future)
```

### VS Code / LSP Extension
Show shadow deps inline as editor decorations. The JSON manifest already has file + line + col, which maps directly to VS Code diagnostic locations.

### `looper doctor`
Runtime verification command that actually checks shadow deps: is the env var set? Is the binary installed? Is the port available? Is the host reachable? Turns the static manifest into a runnable health check for onboarding and debugging.

### Dependency Relationship Graph
Some shadow deps are connected: an env var feeds a connection string which implies a host + port. Capturing `DATABASE_URL -> postgres://host:5432` as a directed relationship makes the HTML graph much more useful and enables impact analysis.

### Incremental Scanning
File hash cache (`.looper-cache`) that tracks `file_path -> content_hash -> findings`. Only re-scan files whose hash changed. `--no-cache` flag to force full scan. The walker should be designed to emit `(path, content_hash)` tuples from Phase 1 to support this.

### Monorepo Support
Per-package manifests, `looper scan packages/api/` scoping, workspace-level rollup report that aggregates findings across packages.
