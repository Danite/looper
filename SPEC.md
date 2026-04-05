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
    drift             Compare code needs vs infrastructure provides
    doctor            Runtime health check — verify shadow deps are satisfied
    generate          Auto-generate infrastructure files from scan results

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

GENERATE OPTIONS:
    <TARGET>               What to generate: env, compose, dockerfile, readme, all
    -o, --output <FILE>    Write to file (default: stdout for preview)
    --force                Overwrite existing files without warning
    --min-confidence <LVL> Minimum confidence for included deps (default: medium)
    --scan-paths <PATHS>   Override scan paths (comma-separated)
    -v, --verbose          Show per-file scanning progress
    --config <FILE>        Path to config file

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

## Infrastructure Drift Detection

The `drift` subcommand compares what the **source code needs** (from scanning) against what the **infrastructure provides** (from docker-compose, .env, Dockerfile), surfacing gaps that cause deployment failures.

No other tool bridges this gap. Config-level tools (like segspec) read infrastructure files but don't analyze source code. Looper reads source code but now also reads infrastructure configs, enabling cross-layer drift detection.

### Category Mapping

| Code needs (from scan) | Infra provides (from config) | Comparison |
|---|---|---|
| env_var: `DATABASE_URL` | `.env`, `.env.example`, compose `environment` | String match on var name |
| binary: `ffmpeg` | Dockerfile `RUN apt-get install` | Package-to-binary mapping |
| port (connect): `:6379` | Compose services (redis image -> 6379) | Port + protocol match |
| port (listen): `:3000` | Compose `ports`, Dockerfile `EXPOSE` | Port number match |
| network: `cache.internal` | Compose service names, `extra_hosts` | Internal hosts only |
| network: `api.stripe.com` | N/A (external) | Classified as external |
| filesystem: `./config.json` | Dockerfile `COPY`/`ADD`, compose volumes | Path prefix match |
| platform | Skipped (no infra counterpart) | -- |

### Infra File Precedence

- **`.env`** checked first (authoritative but gitignored), **`.env.example`** as fallback (committed)
- **`docker-compose.yml`** and **`docker-compose.yaml`** auto-detected
- **`Dockerfile`** auto-detected in project root

### External vs Internal Network Hosts

Network deps are classified as **external** (not infrastructure-managed) if the host contains a known TLD (`.com`, `.io`, `.org`, `.net`, `.dev`) or matches well-known API patterns (`api.*`, `*.amazonaws.com`). Bare hostnames without dots (e.g., `redis`, `cache`) are treated as internal docker-compose service names.

External hosts are shown in a separate section rather than flagged as "missing from infrastructure."

### CLI

```
looper drift [OPTIONS]

OPTIONS:
    --compose <FILE>       Path to docker-compose.yml (default: auto-detect)
    --env <FILE>           Path to .env file (default: auto-detect .env then .env.example)
    --dockerfile <FILE>    Path to Dockerfile (default: auto-detect)
    --fail-on-missing      Exit 1 if code needs something infra doesn't provide (CI gate)
    -f, --format <FMT>     Output format: terminal (default), json
    -o, --output <FILE>    Write to file
    --scan-paths <PATHS>   Override scan paths (default: from config or '.')
```

Exit codes: 0 = no missing items (or `--fail-on-missing` not set), 1 = missing items found (with flag), 2 = no infrastructure files found.

### Terminal Output

```
looper drift — comparing code vs infrastructure

 MISSING FROM INFRASTRUCTURE (code needs, infra doesn't provide):
 ──────────────────────────────────────────────────
  env_var   STRIPE_SECRET_KEY      not in .env or docker-compose
  env_var   JWT_SECRET             not in .env or docker-compose
  binary    ffmpeg                 not in Dockerfile (apt-get/apk)
  port      :6379 (connect)       no redis service in docker-compose

 EXTERNAL (not infrastructure-managed):
 ──────────────────────────────────────────────────
  network   api.stripe.com         external API
  network   sentry.io              external API

 UNUSED IN INFRASTRUCTURE (infra provides, code doesn't use):
 ──────────────────────────────────────────────────
  env_var   OLD_API_KEY            in .env but not referenced in code
  port      :5432 (connect)       postgres in docker-compose but no code connects

 COVERED (code needs, infra provides):
 ──────────────────────────────────────────────────
  env_var   DATABASE_URL           .env line 3
  port      :3000 (listen)        docker-compose ports: "3000:3000"
  binary    node                  Dockerfile FROM node:20

 Summary: 4 missing, 2 external, 2 unused, 3 covered
```

### CI Integration

```yaml
# .github/workflows/shadow-deps.yml
- name: Check infrastructure drift
  run: looper drift --fail-on-missing
```

---

## Runtime Health Check (`looper doctor`)

The `doctor` subcommand runtime-verifies shadow dependencies: is the env var set? Is the binary installed? Is the port reachable? Is the host resolvable? Turns the static scan manifest into a runnable health check.

The killer onboarding tool: new developer clones repo, runs `looper doctor`, sees exactly what's red, fixes it.

### What Gets Checked

| Category | Runtime check | Pass condition | Example |
|---|---|---|---|
| **env_var** | `std::env::var(name)` | Variable set and non-empty | DATABASE_URL -> set / missing |
| **binary** | PATH lookup via `which` | Binary found | ffmpeg -> /usr/local/bin/ffmpeg |
| **port (connect)** | TCP connect with timeout | Connection accepted | :6379 -> reachable / refused |
| **port (listen)** | Bind attempt | Port available | :3000 -> available / in use |
| **network** | DNS resolve + TCP connect | Host resolves | api.stripe.com -> reachable |
| **filesystem** | `Path::exists()` | File/dir exists | ./config.json -> exists / missing |
| **platform** | Skipped | -- | -- |

### Smart handling

- **Secret redaction**: Env vars whose name contains SECRET, KEY, TOKEN, PASSWORD, CREDENTIAL, or AUTH show `set (*****)` instead of the value.
- **Optional env vars**: If `metadata.required = false` (has a default), show "optional (default: 3000)" instead of failing.
- **Dynamic env vars**: Deps named `<dynamic>` (from `process.env[variable]`) are skipped.
- **Port direction**: Read `metadata.direction` to choose connect check vs bind check.
- **Filesystem paths**: Resolved relative to scan root, not CWD.
- **Install suggestions**: Failed binary checks suggest the install command via reverse package-to-binary mapping (e.g., "install: apt-get install imagemagick").

### CLI

```
looper doctor [OPTIONS]

OPTIONS:
    --manifest <FILE>      Use pre-scanned manifest instead of live scan
    --timeout <MS>         Timeout for network/port checks (default: 2000ms)
    --fail-on-error        Exit 1 if any check fails (for CI)
    -f, --format <FMT>     Output format: terminal (default), json
    -o, --output <FILE>    Write to file
    --scan-paths <PATHS>   Override scan paths
    -v, --verbose          Show detailed check output
    --config <FILE>        Path to config file
```

Exit codes: 0 = all pass (or `--fail-on-error` not set), 1 = checks failed (with flag), 2 = no shadow deps found.

### Terminal Output

```
looper doctor — runtime health check

 ENV VARS
  ✓ DATABASE_URL         set
  ✗ STRIPE_SECRET_KEY    missing
  ✓ PORT                 optional (default: 3000)
  ✓ NODE_ENV             set (production)

 BINARIES
  ✓ ffmpeg               /usr/local/bin/ffmpeg
  ✗ convert              not found — install: apt-get install imagemagick
  ✓ git                  /usr/bin/git

 PORTS
  ✓ :5432 (connect)      reachable
  ✗ :6379 (connect)      connection refused
  ✓ :3000 (listen)       available

 NETWORK
  ✓ api.stripe.com       reachable (45ms)
  ✗ internal.service     DNS resolution failed

 FILESYSTEM
  ✓ ./config.json        exists
  ✗ /tmp/cache/          missing

 Summary: 9 passed, 3 failed
```

---

## Infrastructure File Generation (`looper generate`)

The `generate` subcommand auto-generates infrastructure files from scan results. Instead of just reporting what's missing, looper produces the fix.

### Targets

| Target | Output file | What it generates |
|--------|-------------|-------------------|
| `env` | `.env.example` | All detected env vars with placeholders inferred from naming (`*_URL` → `https://`, `*_KEY` → `changeme`, etc.). Groups required vs optional. |
| `compose` | `docker-compose.yml` | App service with listen ports and env vars. Infrastructure services (Redis, Postgres, etc.) from connect-direction ports with protocol hints. |
| `dockerfile` | `Dockerfile` | Base image detected from project markers (`package.json` → `node:20-slim`, etc.). `RUN apt-get install` for detected binaries. `EXPOSE` for listen ports. |
| `readme` | `REQUIREMENTS.md` | Markdown tables for env vars, binaries (with install commands), ports, network endpoints, filesystem paths. |
| `all` | All of the above | Writes each to its default filename. |

### Confidence Filtering

Generators only include `High` and `Medium` confidence deps by default. `Low` confidence findings (dynamic `process.env[variable]`) and `<dynamic>` env vars are excluded to prevent noisy output. Override with `--min-confidence low`.

### File Conflict Handling

- **No `--output`**: Output goes to stdout (preview mode, safe by default).
- **With `--output`**: If the target file exists and `--force` is not set, looper warns and skips.
- **With `--force`**: Overwrites without prompting.
- **`all` target**: Always writes to default filenames. Rejects `--output` (use individual targets instead).

### CLI

```
looper generate <TARGET> [OPTIONS]

ARGUMENTS:
    <TARGET>               env, compose, dockerfile, readme, all

OPTIONS:
    -o, --output <FILE>    Write to file (default: stdout)
    --force                Overwrite existing files
    --min-confidence <LVL> Minimum confidence: low, medium, high (default: medium)
    --scan-paths <PATHS>   Override scan paths (comma-separated)
    -v, --verbose          Show per-file scanning progress
    --config <FILE>        Path to config file
```

Exit codes: 0 = success, 1 = error (invalid target, write failure), 2 = no shadow deps found.

### Service Mapping (compose)

The compose generator maps `protocol_hint` from port findings to Docker images:

| Protocol hint | Service name | Image | Default port |
|---------------|-------------|-------|-------------|
| `redis` | redis | `redis:7` | 6379 |
| `postgres` | db | `postgres:16` | 5432 |
| `mongodb` | mongo | `mongo:7` | 27017 |
| `mysql` | mysql | `mysql:8` | 3306 |
| `amqp` | rabbitmq | `rabbitmq:3-management` | 5672 |
| `nats` | nats | `nats:2` | 4222 |
| `memcached` | memcached | `memcached:1` | 11211 |
| `elasticsearch` | elasticsearch | `elasticsearch:8` | 9200 |

### Base Image Detection (dockerfile)

| Project marker | Base image |
|----------------|-----------|
| `package.json` | `node:20-slim` |
| `requirements.txt` / `pyproject.toml` | `python:3.12-slim` |
| `go.mod` | `golang:1.23-alpine` |
| `Cargo.toml` | `rust:1.82-slim` |
| `Gemfile` | `ruby:3.3-slim` |
| `composer.json` | `php:8.3-cli` |
| (none found) | `debian:bookworm-slim` |

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
│   ├── diff.rs              # Diff two manifest files
│   ├── doctor.rs            # Runtime health check engine
│   ├── drift.rs             # Infrastructure drift engine
│   ├── generate/
│   │   ├── mod.rs           # GenerateTarget enum, dispatch
│   │   ├── env.rs           # .env.example generator
│   │   ├── compose.rs       # docker-compose.yml generator
│   │   ├── dockerfile.rs    # Dockerfile generator
│   │   └── readme.rs        # README requirements section generator
│   └── infra/
│       ├── mod.rs           # InfraManifest type, auto-detection
│       ├── envfile.rs       # .env / .env.example parser
│       ├── compose.rs       # docker-compose.yml parser
│       └── dockerfile.rs    # Dockerfile parser
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
chrono = { version = "0.4", default-features = false, features = ["clock", "serde"] }
ignore = "0.4"              # .gitignore-aware file walking (includes WalkParallel)
colored = "2"               # Terminal colors
rayon = "1.10"              # Parallel file scanning
url = "2"                   # URL parsing for network detector
regex = "1"                 # Pattern matching for connection strings
serde_yaml = "0.9"          # YAML parsing for docker-compose
which = "8"                 # Binary lookup in PATH

[dev-dependencies]
insta = { version = "1", features = ["json", "redactions"] }  # Snapshot testing
tempfile = "3"              # Temp directories for tests
```

Future additions as needed:

```toml
oxc_semantic = "0.123"      # Scope analysis for deeper binding resolution
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

### Phase 5 — Infrastructure Drift Detection
- Infra parsers (`infra/envfile.rs`) — `.env` / `.env.example` parser with precedence
- Infra parsers (`infra/compose.rs`) — docker-compose.yml parser (services, ports, environment, image, volumes)
- Infra parsers (`infra/dockerfile.rs`) — Dockerfile parser (FROM, RUN install, EXPOSE, COPY/ADD)
- Auto-detection of infra files in project root
- Drift engine (`drift.rs`) — compare ScanManifest vs InfraManifest
- Image-to-service mapping (redis image -> provides redis:6379)
- Package-to-binary mapping (imagemagick -> convert, mogrify)
- External host heuristic (`.com`/`.io` = external, bare names = internal)
- Skip Platform category in drift (no infra counterpart)
- `drift` subcommand with `--fail-on-missing` for CI gating
- Terminal and JSON output for drift report (missing/external/unused/covered)
- Exit codes: 0 = clean, 1 = missing (with flag), 2 = no infra files

**Milestone:** `looper drift --fail-on-missing` gates PRs for infrastructure completeness. Cross-layer drift detection between code and infra.

### Phase 6 — Runtime Health Check (`looper doctor`)
- Doctor engine (`doctor.rs`) — check env vars set, binaries in PATH, ports reachable, hosts resolvable, files exist
- Secret redaction for sensitive env var values
- Optional env var handling (skip failure for vars with defaults)
- Port direction-aware checks (connect vs listen from metadata)
- Binary install suggestions via reverse package-to-binary mapping
- `doctor` subcommand with `--fail-on-error` for CI, `--manifest` for pre-scanned, `--timeout` for network checks
- Terminal output with checkmarks/crosses, JSON output
- Exit codes: 0 = all pass, 1 = checks failed (with flag), 2 = no deps found

**Milestone:** `looper doctor` verifies a development environment is fully set up. The onboarding health check.

---

## Expansion Roadmap (post-v3)

### Multi-Language Support
Python (`os.environ`, `subprocess.run`, `open()`), Go (`os.Getenv`, `exec.Command`, `http.ListenAndServe`), Ruby (`ENV[]`, `system()`, `File.read`). The `Detector` trait already outputs a language-agnostic `Vec<ShadowDep>`. Add language backends:

```
Detector (language-agnostic output: Vec<ShadowDep>)
  └── JsDetector (oxc AST)
  └── PyDetector (tree-sitter AST, future)
  └── GoDetector (tree-sitter AST, future)
```

### Kubernetes Support
Extend infra parsers to read Kubernetes manifests (Deployment, Service, ConfigMap, Secret) for drift detection against k8s-native projects.

### VS Code / LSP Extension
Show shadow deps inline as editor decorations. The JSON manifest already has file + line + col, which maps directly to VS Code diagnostic locations.

### Dependency Relationship Graph
Some shadow deps are connected: an env var feeds a connection string which implies a host + port. Capturing `DATABASE_URL -> postgres://host:5432` as a directed relationship makes the HTML graph much more useful and enables impact analysis.

### Incremental Scanning
File hash cache (`.looper-cache`) that tracks `file_path -> content_hash -> findings`. Only re-scan files whose hash changed. `--no-cache` flag to force full scan.

### Monorepo Support
Per-package manifests, `looper scan packages/api/` scoping, workspace-level rollup report that aggregates findings across packages.
