#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/acceptance-fixture.sh <fixture>

Fixtures:
  smoke-python-search
  smoke-rust-cli
EOF
}

if [ "$#" -ne 1 ]; then
  usage
  exit 1
fi

fixture="$1"
case "$fixture" in
  smoke-python-search|smoke-rust-cli) ;;
  *)
    usage
    exit 1
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$repo_root/examples/$fixture"
workspace="$(mktemp -d "${TMPDIR:-/tmp}/autoloop-acceptance.${fixture}.XXXXXX")"
binary="$repo_root/target/debug/autoloop"

printf 'Building autoloop binary...\n'
cargo build --quiet --manifest-path "$repo_root/Cargo.toml" --bin autoloop

printf 'Preparing fixture workspace at %s\n' "$workspace"
cp -R "$fixture_root"/. "$workspace"

(
  cd "$workspace"
  git init -q
  git config user.name "AutoLoop Acceptance"
  git config user.email "acceptance@autoloop.local"
  git add .
  git commit -q -m "initial fixture"

  printf 'Installing Codex integration...\n'
  "$binary" install codex >/dev/null
  test -f "AGENTS.md"
  test -f ".agents/skills/autoloop-run/SKILL.md"

  printf 'Running init --verify...\n'
  init_json="$("$binary" init --json --verify)"
  printf '%s\n' "$init_json" | grep -q '"healthy": true'

  printf 'Replacing config with the default template...\n'
  cat > ".autoloop/config.toml" <<'EOF'
# autoloop v0 template
strictness = "advisory"

[metric]
name = "latency_p95"
direction = "lower"
unit = "ms"

[eval]
command = "echo 'METRIC latency_p95=42.3'"
timeout = 300
format = "metric_lines"
retries = 1

[confidence]
min_experiments = 3
keep_threshold = 1.0
rerun_threshold = 2.0

[git]
enabled = true
commit_prefix = "experiment:"
EOF

  printf 'Running doctor to confirm the broken config is detected...\n'
  doctor_json="$("$binary" doctor --json)"
  printf '%s\n' "$doctor_json" | grep -q '"healthy": false'

  printf 'Running doctor --fix...\n'
  doctor_fix_json="$("$binary" doctor --json --fix)"
  printf '%s\n' "$doctor_fix_json" | grep -q '"healthy": true'
  printf '%s\n' "$doctor_fix_json" | grep -q '"applied": true'

  printf 'Recording baseline...\n'
  baseline_json="$("$binary" baseline --json)"
  printf '%s\n' "$baseline_json" | grep -q '"metric"'

  find . -type d \( -name "__pycache__" -o -name ".pytest_cache" -o -name ".mypy_cache" \) -prune -exec rm -rf {} +
  find . -type f \( -name "*.pyc" -o -name "*.pyo" \) -delete

  if [ -n "$(git status --short)" ]; then
    printf 'Committing setup-only integration files to start the manual run clean...\n'
    for path in .gitignore AGENTS.md .agents; do
      if [ -e "$path" ]; then
        git add "$path"
      fi
    done
    if ! git diff --cached --quiet; then
      git commit -q -m "prepare autoloop integration"
    fi
  fi

  if [ -n "$(git status --short)" ]; then
    printf 'Prepared workspace is still dirty after setup normalization.\n' >&2
    git status --short >&2
    exit 1
  fi
)

cat <<EOF

Automated acceptance checks passed for $fixture.

Workspace:
  $workspace

Manual bounded run:
  1. Open the workspace above in Codex.
  2. The repo is intentionally left with a clean \`git status\` so the loop starts from a non-experiment baseline.
  3. Invoke the installed \`autoloop-run\` wrapper with:
     Use \`autoloop-run\` to reduce the benchmark latency in this repo. Keep behavior unchanged. Use at most 5 experiments, prefer fully automatic setup, and ask me only if you are genuinely blocked.
  4. After the run, inspect:
     - autoloop status --all
     - autoloop learn --all
     - git log --oneline --decorate --graph --all

Cleanup:
  rm -rf "$workspace"
EOF
