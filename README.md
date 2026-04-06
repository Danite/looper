# looper

Find the dependencies your package manager doesn't track.

`looper` statically analyzes JavaScript/TypeScript code and extracts implicit dependencies: env vars, filesystem paths, network hosts, system binaries, ports, and platform assumptions, into a structured JSON manifest.

```
$ looper .

looper v0.1.0 — scanned 142 files in 48ms

 ENV VARS (7 found)
 ──────────────────────────────────────────────────
  DATABASE_URL        src/db.ts:12         high
  STRIPE_SECRET_KEY   src/billing.ts:8     high
  PORT                src/server.ts:5      high

 NETWORK (3 found)
 ──────────────────────────────────────────────────
  api.stripe.com      src/billing.ts:22    high
  sentry.io           src/errors.ts:5      high

 BINARIES (1 found)
 ──────────────────────────────────────────────────
  ffmpeg              src/video.ts:15      high

 Summary: 15 shadow deps across 6 categories
```

## Install

```bash
cargo install looper-cli
```

## What It Finds

| Category | Example | Patterns |
|----------|---------|----------|
| **Env vars** | `process.env.DATABASE_URL` | `process.env`, `import.meta.env`, `Deno.env.get`, `Bun.env` |
| **Filesystem** | `fs.readFileSync("./config.json")` | `fs.*`, `path.join`, `Deno.readTextFile`, `Bun.file` |
| **Network** | `fetch("https://api.stripe.com")` | `fetch`, `axios`, `http.request`, `new URL`, `WebSocket` |
| **Binaries** | `exec("ffmpeg -i input.mp4")` | `exec`, `spawn`, `execa`, zx `$` templates |
| **Ports** | `app.listen(3000)` | `.listen()`, `net.createConnection`, connection strings |
| **Platform** | `process.platform === "darwin"` | Unix paths, Windows commands, `os.*`, `path.sep` |

Each finding includes a confidence level (`high`, `medium`, `low`) for filtering by reliability.

## Usage

```bash
looper .                                    # scan current directory
looper src/ lib/                            # scan specific paths
looper . --format json -o shadow-deps.json  # JSON manifest
looper . --format sarif -o report.sarif     # SARIF for GitHub Code Scanning
looper . --format html -o report.html       # self-contained HTML report

looper check                                # compare scan against committed manifest
looper check --update                       # update the manifest with current scan
looper diff old.json new.json               # diff two manifests
looper drift --fail-on-missing              # fail if code needs something infra doesn't provide
looper doctor                               # runtime check: are env vars set? binaries installed?
looper generate env -o .env.example         # generate infra files from scan results
looper init                                 # create config file for project setup
```

Run `looper --help` or `looper <command> --help` for full option reference.

## CI

```yaml
# .github/workflows/shadow-deps.yml
name: Shadow Deps
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install looper-cli
      - run: looper check
      - run: looper drift --fail-on-missing
```

When a PR adds `process.env.NEW_KEY` without updating the manifest, the check fails:

```
error: 1 undeclared shadow dependency found

  + env_var: NEW_KEY  (src/feature.ts:42)

Update the manifest:  looper check --update
```

## Configuration

`looper init` generates a `looper.config.json` in your project root. See `looper init --help` and [USAGE.md](USAGE.md) for details.

## License

MIT
