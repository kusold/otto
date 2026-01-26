#!/usr/bin/env bash
# Script to check test coverage for otto-agent-claude crate
# This should be run from within the Nix dev shell

set -e

echo "Installing cargo-llvm-cov if not present..."
if ! cargo llvm-cov --version &>/dev/null; then
    cargo install cargo-llvm-cov
fi

echo "Running coverage report for otto-agent-claude..."
cargo llvm-cov --package otto-agent-claude --html --output-dir coverage

echo ""
echo "Coverage report generated in coverage/index.html"
echo "Opening coverage report..."
xdg-open coverage/index.html 2>/dev/null || open coverage/index.html 2>/dev/null || echo "Please open coverage/index.html in your browser"
