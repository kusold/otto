.PHONY: test test-integration help

help:
	@echo "Available targets:"
	@echo "  make test           - Run all tests"
	@echo "  make test-integration - Run integration tests for otto done"
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
