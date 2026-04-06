#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECTS_FILE="$SCRIPT_DIR/projects.json"
REPOS_DIR="$SCRIPT_DIR/repos"
RESULTS_DIR="$SCRIPT_DIR/results"
EXPECTED_DIR="$SCRIPT_DIR/expected"
REPORTS_DIR="$SCRIPT_DIR/reports"

# --- Colors -----------------------------------------------------------

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
YELLOW=$'\033[0;33m'
BLUE=$'\033[0;34m'
BOLD=$'\033[1m'
DIM=$'\033[2m'
RESET=$'\033[0m'

info()  { printf "${BLUE}info:${RESET} %s\n" "$*"; }
ok()    { printf "${GREEN}  ok:${RESET} %s\n" "$*"; }
warn()  { printf "${YELLOW}warn:${RESET} %s\n" "$*" >&2; }
err()   { printf "${RED}error:${RESET} %s\n" "$*" >&2; }

# --- Dependency checks ------------------------------------------------

find_looper() {
  if command -v looper &>/dev/null; then
    command -v looper
  elif [[ -x "$PROJECT_ROOT/target/release/looper" ]]; then
    echo "$PROJECT_ROOT/target/release/looper"
  elif [[ -x "$PROJECT_ROOT/target/debug/looper" ]]; then
    echo "$PROJECT_ROOT/target/debug/looper"
  else
    return 1
  fi
}

check_deps() {
  local missing=0

  if ! command -v jq &>/dev/null; then
    err "jq is required but not found. Install: brew install jq"
    missing=1
  fi

  if ! command -v git &>/dev/null; then
    err "git is required but not found"
    missing=1
  fi

  if ! find_looper &>/dev/null; then
    err "looper binary not found. Run: cargo build --release"
    missing=1
  fi

  if [[ $missing -ne 0 ]]; then
    exit 2
  fi

  LOOPER="$(find_looper)"
}

# --- Project helpers --------------------------------------------------

list_project_names() {
  jq -r '.projects[].name' "$PROJECTS_FILE"
}

get_project_field() {
  local name="$1" field="$2"
  jq -r --arg n "$name" --arg f "$field" \
    '.projects[] | select(.name == $n) | .[$f] // empty' "$PROJECTS_FILE"
}

get_scan_paths() {
  local name="$1"
  local paths
  paths=$(jq -r --arg n "$name" \
    '.projects[] | select(.name == $n) | .scan_paths // ["."] | .[]' "$PROJECTS_FILE")
  echo "$paths"
}

get_looper_args() {
  local name="$1"
  jq -r --arg n "$name" \
    '.projects[] | select(.name == $n) | .looper_args // [] | .[]' "$PROJECTS_FILE"
}

validate_project() {
  local name="$1"
  local repo
  repo=$(get_project_field "$name" "repo")
  if [[ -z "$repo" ]]; then
    err "unknown project: $name"
    echo "Available projects:" >&2
    list_project_names | sed 's/^/  /' >&2
    exit 1
  fi
}

is_tag_ref() {
  local ref="$1"
  [[ "$ref" =~ ^v[0-9] ]] || [[ "$ref" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]]
}

# --- Subcommands ------------------------------------------------------

cmd_setup() {
  local filter="${1:-}"

  mkdir -p "$REPOS_DIR"

  for name in $(list_project_names); do
    if [[ -n "$filter" && "$name" != "$filter" ]]; then
      continue
    fi

    local repo ref
    repo=$(get_project_field "$name" "repo")
    ref=$(get_project_field "$name" "ref")
    local target="$REPOS_DIR/$name"

    if [[ -d "$target/.git" ]]; then
      info "$name: already cloned, checking out $ref"
      (cd "$target" && git checkout --quiet "$ref" 2>/dev/null) || {
        info "$name: fetching ref $ref"
        (cd "$target" && git fetch --quiet origin "$ref" && git checkout --quiet "$ref")
      }
    else
      info "$name: cloning $repo @ $ref"
      if is_tag_ref "$ref"; then
        git clone --branch "$ref" --depth 1 --quiet "$repo" "$target"
      else
        git clone --filter=blob:none --quiet "$repo" "$target"
        (cd "$target" && git checkout --quiet "$ref")
      fi
    fi

    ok "$name: ready at $(cd "$target" && git rev-parse --short HEAD)"
  done
}

cmd_scan() {
  local filter="${1:-}"

  mkdir -p "$RESULTS_DIR"

  for name in $(list_project_names); do
    if [[ -n "$filter" && "$name" != "$filter" ]]; then
      continue
    fi

    local repo_path="$REPOS_DIR/$name"
    if [[ ! -d "$repo_path" ]]; then
      warn "$name: not cloned yet, run 'bench.sh setup $name' first"
      continue
    fi

    local scan_paths=()
    while IFS= read -r p; do
      scan_paths+=("$p")
    done < <(get_scan_paths "$name")

    local extra_args=()
    while IFS= read -r a; do
      [[ -n "$a" ]] && extra_args+=("$a")
    done < <(get_looper_args "$name")

    local output="$RESULTS_DIR/$name.json"
    local stderr_tmp
    stderr_tmp=$(mktemp)

    info "$name: scanning..."
    if (cd "$repo_path" && "$LOOPER" scan --format json \
        -o "$output" \
        ${extra_args[@]+"${extra_args[@]}"} \
        "${scan_paths[@]}") 2>"$stderr_tmp"; then
      local count
      count=$(jq '.deps | length' "$output")
      ok "$name: $count findings -> results/$name.json"
    else
      if [[ -f "$output" ]]; then
        local count
        count=$(jq '.deps | length' "$output" 2>/dev/null || echo "0")
        ok "$name: $count findings (scan had warnings) -> results/$name.json"
      else
        err "$name: scan failed"
      fi
    fi
    if [[ -s "$stderr_tmp" ]]; then
      warn "$name: scan produced warnings (see below)"
      cat "$stderr_tmp" >&2
    fi
    rm -f "$stderr_tmp"
  done
}

cmd_compare() {
  local filter="${1:-}"
  local has_regressions=0

  for name in $(list_project_names); do
    if [[ -n "$filter" && "$name" != "$filter" ]]; then
      continue
    fi

    local result="$RESULTS_DIR/$name.json"
    local approved="$EXPECTED_DIR/$name/approved.json"
    local gaps="$EXPECTED_DIR/$name/known-gaps.json"

    if [[ ! -f "$result" ]]; then
      warn "$name: no scan results, run 'bench.sh scan $name' first"
      continue
    fi

    local ref
    ref=$(get_project_field "$name" "ref")
    printf "\n${BOLD}## %s${RESET} ${DIM}(%s)${RESET}\n\n" "$name" "$ref"

    local detected
    detected=$(jq '.deps | length' "$result")

    if [[ ! -f "$approved" ]]; then
      printf "  Detected findings: %s\n" "$detected"
      printf "  ${YELLOW}No approved.json yet -- run 'bench.sh approve %s' after review${RESET}\n" "$name"
      continue
    fi

    # Use looper diff for comparison
    local diff_json
    diff_json=$("$LOOPER" diff "$approved" "$result" --format json 2>/dev/null) || {
      err "$name: looper diff failed"
      continue
    }

    local approved_count added removed changed
    approved_count=$(jq '.deps | length' "$approved")
    added=$(echo "$diff_json" | jq '.added | length')
    removed=$(echo "$diff_json" | jq '.removed | length')
    changed=$(echo "$diff_json" | jq '.changed | length')
    local matched=$(( (approved_count - removed - changed) > 0 ? approved_count - removed - changed : 0 ))

    printf "  Approved findings:  %s\n" "$approved_count"
    printf "  Detected findings:  %s\n" "$detected"
    printf "\n"
    printf "    Matched:          %s  ${DIM}(stable true positives)${RESET}\n" "$matched"
    if [[ "$changed" -gt 0 ]]; then
      printf "    Changed:          ${YELLOW}%s${RESET}  ${DIM}(confidence/locations differ)${RESET}\n" "$changed"
    else
      printf "    Changed:          %s\n" "$changed"
    fi
    if [[ "$added" -gt 0 ]]; then
      printf "    New findings:     ${GREEN}%s${RESET}  ${DIM}(review needed)${RESET}\n" "$added"
    else
      printf "    New findings:     %s\n" "$added"
    fi
    if [[ "$removed" -gt 0 ]]; then
      printf "    Regressions:      ${RED}%s${RESET}  ${DIM}(previously detected, now missing)${RESET}\n" "$removed"
      has_regressions=1
    else
      printf "    Regressions:      %s\n" "$removed"
    fi

    # Known gaps analysis
    if [[ -f "$gaps" ]]; then
      local total_gaps now_detected still_missing
      total_gaps=$(jq '.gaps | length' "$gaps")

      # Check which gap ids now appear in the scan results
      now_detected=$(jq --slurpfile result "$result" \
        '[.gaps[].id] as $gap_ids |
         [$result[0].deps[].id] as $result_ids |
         [$gap_ids[] | select(. as $gid | $result_ids | any(. == $gid))] | length' "$gaps")
      still_missing=$((total_gaps - now_detected))

      printf "\n"
      printf "  Known gaps:         %s\n" "$total_gaps"
      if [[ "$now_detected" -gt 0 ]]; then
        printf "    Now detected:     ${GREEN}%s${RESET}  ${DIM}(gap resolved!)${RESET}\n" "$now_detected"
      else
        printf "    Now detected:     %s\n" "$now_detected"
      fi
      printf "    Still missing:    %s\n" "$still_missing"

      local coverage_total=$((approved_count + still_missing))
      if [[ "$coverage_total" -gt 0 ]]; then
        local pct
        pct=$(awk "BEGIN { printf \"%.1f\", ($approved_count / $coverage_total) * 100 }")
        printf "\n  ${BOLD}Coverage: %s/%s = %s%%${RESET}\n" "$approved_count" "$coverage_total" "$pct"
      fi
    fi

    # Show details for regressions
    if [[ "$removed" -gt 0 ]]; then
      printf "\n  ${RED}Regressions:${RESET}\n"
      echo "$diff_json" | jq -r '.removed[] | "    - \(.category)::\(.name)"'
    fi

    # Show details for new findings
    if [[ "$added" -gt 0 ]]; then
      printf "\n  ${GREEN}New findings:${RESET}\n"
      echo "$diff_json" | jq -r '.added[] | "    + \(.category)::\(.name)"'
    fi
  done

  printf "\n"
  if [[ "$has_regressions" -ne 0 ]]; then
    err "regressions detected"
    return 1
  fi
  return 0
}

cmd_approve() {
  local name="${1:-}"
  if [[ -z "$name" ]]; then
    err "usage: bench.sh approve <project>"
    exit 1
  fi
  validate_project "$name"

  local result="$RESULTS_DIR/$name.json"
  if [[ ! -f "$result" ]]; then
    err "no scan results for $name, run 'bench.sh scan $name' first"
    exit 1
  fi

  mkdir -p "$EXPECTED_DIR/$name"
  local approved="$EXPECTED_DIR/$name/approved.json"

  # Replace volatile fields with stable placeholders
  jq '{
    schema_version: .schema_version,
    tool_version: "baseline",
    scanned_at: "approved",
    root: ".",
    files_scanned: .files_scanned,
    scan_duration_ms: 0,
    deps: .deps
  }' "$result" > "$approved"

  local count
  count=$(jq '.deps | length' "$approved")
  ok "$name: approved $count findings -> expected/$name/approved.json"

  local gaps="$EXPECTED_DIR/$name/known-gaps.json"
  if [[ ! -f "$gaps" ]]; then
    # Create empty known-gaps template
    jq -n --arg name "$name" '{
      project: $name,
      gaps: []
    }' > "$gaps"
    info "$name: created empty known-gaps.json (add entries manually after reviewing source)"
  fi
}

cmd_report() {
  mkdir -p "$REPORTS_DIR"
  local report="$REPORTS_DIR/summary.md"
  local now
  now=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

  {
    echo "# Sandbox Validation Report"
    echo ""
    echo "Generated: $now"
    echo ""

    local total_approved=0
    local total_detected=0
    local total_gaps=0
    local total_resolved=0
    local total_regressions=0

    # Collect blocked_by stats
    local blocked_by_tmp
    blocked_by_tmp=$(mktemp)

    for name in $(list_project_names); do
      local result="$RESULTS_DIR/$name.json"
      local approved="$EXPECTED_DIR/$name/approved.json"
      local gaps="$EXPECTED_DIR/$name/known-gaps.json"
      local ref
      ref=$(get_project_field "$name" "ref")

      echo "## $name ($ref)"
      echo ""

      if [[ ! -f "$result" ]]; then
        echo "*No scan results available.*"
        echo ""
        continue
      fi

      local detected
      detected=$(jq '.deps | length' "$result")
      total_detected=$((total_detected + detected))

      if [[ ! -f "$approved" ]]; then
        echo "- Detected findings: $detected"
        echo "- **No approved baseline yet**"
        echo ""
        continue
      fi

      local diff_json approved_count added removed changed matched
      diff_json=$("$LOOPER" diff "$approved" "$result" --format json 2>/dev/null) || true
      approved_count=$(jq '.deps | length' "$approved")
      total_approved=$((total_approved + approved_count))

      added=$(echo "$diff_json" | jq '.added | length' 2>/dev/null || echo 0)
      removed=$(echo "$diff_json" | jq '.removed | length' 2>/dev/null || echo 0)
      changed=$(echo "$diff_json" | jq '.changed | length' 2>/dev/null || echo 0)
      matched=$(( (approved_count - removed - changed) > 0 ? approved_count - removed - changed : 0 ))
      total_regressions=$((total_regressions + removed))

      echo "| Metric | Count |"
      echo "|--------|------:|"
      echo "| Approved | $approved_count |"
      echo "| Detected | $detected |"
      echo "| Matched | $matched |"
      echo "| Changed | $changed |"
      echo "| New findings | $added |"
      echo "| Regressions | $removed |"

      if [[ -f "$gaps" ]]; then
        local gap_count now_detected still_missing
        gap_count=$(jq '.gaps | length' "$gaps")
        now_detected=$(jq --slurpfile r "$result" \
          '[.gaps[].id] as $gids |
           [$r[0].deps[].id] as $rids |
           [$gids[] | select(. as $g | $rids | any(. == $g))] | length' "$gaps")
        still_missing=$((gap_count - now_detected))
        total_gaps=$((total_gaps + still_missing))
        total_resolved=$((total_resolved + now_detected))

        echo "| Known gaps | $gap_count |"
        echo "| Gaps resolved | $now_detected |"
        echo "| Gaps remaining | $still_missing |"

        local coverage_total=$((approved_count + still_missing))
        if [[ "$coverage_total" -gt 0 ]]; then
          local pct
          pct=$(awk "BEGIN { printf \"%.1f\", ($approved_count / $coverage_total) * 100 }")
          echo "| **Coverage** | **${pct}%** |"
        fi

        # Collect blocked_by stats (only from non-empty gap lists)
        jq -r 'select(.gaps | length > 0) | .gaps[].blocked_by // "unclassified"' "$gaps" >> "$blocked_by_tmp"
      fi

      echo ""
    done

    # Aggregate summary
    echo "---"
    echo ""
    echo "## Aggregate Summary"
    echo ""
    local total_coverage_denom=$((total_approved + total_gaps))
    if [[ "$total_coverage_denom" -gt 0 ]]; then
      local total_pct
      total_pct=$(awk "BEGIN { printf \"%.1f\", ($total_approved / $total_coverage_denom) * 100 }")
      echo "- **Overall coverage**: $total_approved / $total_coverage_denom = **${total_pct}%**"
    fi
    echo "- **Total regressions**: $total_regressions"
    echo "- **Gaps resolved this run**: $total_resolved"
    echo ""

    # blocked_by breakdown
    if [[ -s "$blocked_by_tmp" ]]; then
      echo "### Gap Root Causes"
      echo ""
      echo "| Blocked By | Count |"
      echo "|------------|------:|"
      sort "$blocked_by_tmp" | uniq -c | sort -rn | while read -r count reason; do
        echo "| $reason | $count |"
      done
      echo ""
    fi

    rm -f "$blocked_by_tmp"

  } > "$report"

  ok "report written to reports/summary.md"
}

cmd_add() {
  local url="${1:-}"
  local name="${2:-}"
  local ref="${3:-HEAD}"

  if [[ -z "$url" ]]; then
    err "usage: bench.sh add <repo-url> [name] [ref]"
    exit 1
  fi

  if [[ -z "$name" ]]; then
    name=$(basename "$url" .git)
  fi

  # Check for duplicates
  if jq -e --arg n "$name" '.projects[] | select(.name == $n)' "$PROJECTS_FILE" &>/dev/null; then
    err "project '$name' already exists in projects.json"
    exit 1
  fi

  # If ref is HEAD, resolve it
  if [[ "$ref" == "HEAD" ]]; then
    info "resolving HEAD for $url..."
    ref=$(git ls-remote "$url" HEAD | cut -f1 | head -c 12)
    if [[ -z "$ref" ]]; then
      err "could not resolve HEAD for $url"
      exit 1
    fi
  fi

  # Add to projects.json
  local tmp
  tmp=$(mktemp)
  jq --arg name "$name" --arg repo "$url" --arg ref "$ref" \
    '.projects += [{"name": $name, "repo": $repo, "ref": $ref, "description": ""}]' \
    "$PROJECTS_FILE" > "$tmp"
  mv -f "$tmp" "$PROJECTS_FILE"

  ok "added $name ($url @ $ref)"
  info "run 'bench.sh setup $name' to clone"
}

cmd_clean() {
  info "removing repos/ and results/"
  rm -rf "$REPOS_DIR" "$RESULTS_DIR" "$REPORTS_DIR"
  ok "clean"
}

cmd_help() {
  cat <<EOF
${BOLD}bench.sh${RESET} -- real-world validation harness for looper

${BOLD}USAGE${RESET}
  bench.sh <command> [args]

${BOLD}COMMANDS${RESET}
  setup   [project]   Clone/update repos from projects.json
  scan    [project]   Run looper against repos, save JSON results
  compare [project]   Compare results against approved baselines
  approve <project>   Promote scan results to approved baseline
  report              Generate aggregate markdown report
  add     <url> [name] [ref]  Add a new project to the registry
  clean               Remove repos/, results/, reports/
  help                Show this help

${BOLD}WORKFLOW${RESET}
  1. bench.sh setup           # clone all projects
  2. bench.sh scan            # scan all projects
  3. bench.sh approve <name>  # review results, then approve
  4. bench.sh compare         # after changes, check for regressions
  5. bench.sh report          # generate summary report

${BOLD}PREREQUISITES${RESET}
  jq, git, looper (cargo build --release)
EOF
}

# --- Main dispatch ----------------------------------------------------

main() {
  local cmd="${1:-help}"
  shift || true

  if [[ "$cmd" != "help" ]]; then
    check_deps
  fi

  case "$cmd" in
    setup)   cmd_setup "$@" ;;
    scan)    cmd_scan "$@" ;;
    compare) cmd_compare "$@" ;;
    approve) cmd_approve "$@" ;;
    report)  cmd_report "$@" ;;
    add)     cmd_add "$@" ;;
    clean)   cmd_clean "$@" ;;
    help|-h|--help) cmd_help ;;
    *)
      err "unknown command: $cmd"
      cmd_help
      exit 1
      ;;
  esac
}

main "$@"
