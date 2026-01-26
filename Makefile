.PHONY: test test-integration test-cov test-cov-html test-cov-open help

help:
	@echo "Available targets:"
	@echo "  make test           - Run all tests"
	@echo "  make test-integration - Run integration tests for otto done"
	@echo "  make test-cov       - Run tests with coverage (terminal output)"
	@echo "  make test-cov-html  - Run tests with coverage (HTML report)"
	@echo "  make test-cov-open  - Run tests with coverage and open HTML report"
	@echo "  make help           - Show this help message"

test: test-integration
	@echo "All tests passed!"

test-integration:
	@echo "Running otto done integration tests..."
	@echo ""
	@./tests/test-otto-done-args.sh
	@echo ""
	@./tests/test-otto-done-git-validation.sh
	@echo ""
	@./tests/test-otto-done-beads.sh
	@echo ""
	@./tests/test-otto-done-exit.sh
	@echo ""
	@./tests/test-otto-done-edge-cases.sh
	@echo ""
	@echo "✓ All integration tests passed!"

test-cov:
	@echo "Running tests with coverage..."
	@cargo llvm-cov --workspace || (echo "" && echo "⚠️  Coverage requires llvm-tools-preview component:" && echo "   If using rustup: rustup component add llvm-tools-preview" && echo "   If using Nix: See RUST-Nix.md for coverage setup instructions" && exit 1)

test-cov-html:
	@echo "Running tests with coverage (HTML report)..."
	@cargo llvm-cov --workspace --html --output-dir coverage || (echo "" && echo "⚠️  Coverage requires llvm-tools-preview component:" && echo "   If using rustup: rustup component add llvm-tools-preview" && echo "   If using Nix: See RUST-Nix.md for coverage setup instructions" && exit 1)

test-cov-open: test-cov-html
	@echo "Opening coverage report..."
	@cargo llvm-cov --workspace --html --output-dir coverage --open
