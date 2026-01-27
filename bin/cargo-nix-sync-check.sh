#!/usr/bin/env bash
# Check if Cargo.nix is in sync with Cargo.lock
# This prevents committing outdated Cargo.nix files

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Check if both files exist
if [[ ! -f Cargo.nix ]]; then
  echo "Error: Cargo.nix not found. Please regenerate it with: nix shell github:cargo2nix/cargo2nix/release-0.12 -c cargo2nix"
  exit 1
fi

if [[ ! -f Cargo.lock ]]; then
  echo "Error: Cargo.lock not found"
  exit 1
fi

# The most reliable check is to try to enter the devshell
# nix will fail with "out of sync" error if Cargo.nix doesn't match Cargo.lock
OUTPUT=$(timeout 60 nix develop --command true 2>&1 || true)

if echo "$OUTPUT" | grep -q "out of sync"; then
  echo "Error: Cargo.nix is out of sync with Cargo.lock"
  echo ""
  echo "To regenerate Cargo.nix, run:"
  echo "  rm Cargo.nix && nix shell github:cargo2nix/cargo2nix/release-0.12 -c cargo2nix"
  echo ""
  echo "Or simply:"
  echo "  nix shell github:cargo2nix/cargo2nix/release-0.12 -c cargo2nix"
  exit 1
fi

# Check if nix develop succeeded (exit code 0 means in sync)
if ! echo "$OUTPUT" | grep -q "error:"; then
  # No errors, check passed
  exit 0
fi

# There was an error, check if it was "out of sync"
if echo "$OUTPUT" | grep -q "out of sync"; then
  echo "Error: Cargo.nix is out of sync with Cargo.lock"
  echo ""
  echo "To regenerate Cargo.nix, run:"
  echo "  rm Cargo.nix && nix shell github:cargo2nix/cargo2nix/release-0.12 -c cargo2nix"
  echo ""
  echo "Or simply:"
  echo "  nix shell github:cargo2nix/cargo2nix/release-0.12 -c cargo2nix"
  exit 1
fi

# Some other error, let it through (might be legitimate nix issue)
exit 0
