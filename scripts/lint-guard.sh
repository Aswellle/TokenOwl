#!/usr/bin/env bash
# Pre-commit lint guard for the clippy lints our local toolchain is too old to
# know about. CI runs stable (currently 1.99); the local baseline is 1.94, so
# a green local `cargo clippy` can still fail CI on newer lints.
#
# Usage: bash scripts/lint-guard.sh   (run from the repository root)
set -euo pipefail

cd "$(dirname "$0")/.."
CRATE=apps/desktop/src-tauri
fail=0

check() {
  local name="$1" pattern="$2"
  local hits
  hits=$(grep -rnE "$pattern" "$CRATE/src" "$CRATE/tests" --include="*.rs" || true)
  if [ -n "$hits" ]; then
    echo "✗ $name"
    echo "$hits" | sed 's/^/    /'
    fail=1
  else
    echo "✓ $name"
  fi
}

# clippy::assert_is_empty (stable 1.99): positive is_empty asserts hide the value
check "no positive assert!(x.is_empty())" \
  'assert!\([A-Za-z_][A-Za-z0-9_.\[\]"]*\.is_empty\(\)\);'

# clippy::redundant_closure_for_method_calls on common iterator helpers
check "no redundant |x| x.len() closures" \
  '\.map(_or)?\([^,]*\|[a-z]+\| [a-z]+\.len\(\)\)'

exit $fail
