.PHONY: test test-cov test-cov-html test-cov-open install release release-patch release-minor release-major help

help:
	@echo "Available targets:"
	@echo "  make test           - Run all tests"
	@echo "  make test-cov       - Run tests with coverage (terminal output)"
	@echo "  make test-cov-html  - Run tests with coverage (HTML report)"
	@echo "  make test-cov-open  - Run tests with coverage and open HTML report"
	@echo "  make install        - Build and install binary to /home/mike/bin"
	@echo "  make release        - Run tests and create/push git tag (usage: make release VERSION=1.0.0)"
	@echo "  make release-patch  - Bump patch version, create tag, and push (e.g., 0.1.0 -> 0.1.1)"
	@echo "  make release-minor  - Bump minor version, create tag, and push (e.g., 0.1.0 -> 0.2.0)"
	@echo "  make release-major  - Bump major version, create tag, and push (e.g., 0.1.0 -> 1.0.0)"
	@echo "  make help           - Show this help message"

test:
	@echo "Running Rust unit tests..."
	@echo ""
	@echo "Testing otto..."
	@cargo test -p otto
	@echo ""
	@echo "Testing otto-core..."
	@cargo test -p otto-core
	@echo ""
	@echo "Testing otto-agent-claude..."
	@cargo test -p otto-agent-claude --lib
	@echo ""
	@echo "✓ All tests passed!"

test-cov:
	@echo "Running tests with coverage..."
	@cargo llvm-cov --workspace || (echo "" && echo "⚠️  Coverage requires llvm-tools-preview component:" && echo "   If using rustup: rustup component add llvm-tools-preview" && echo "   If using Nix: See RUST-Nix.md for coverage setup instructions" && exit 1)

test-cov-html:
	@echo "Running tests with coverage (HTML report)..."
	@cargo llvm-cov --workspace --html --output-dir coverage || (echo "" && echo "⚠️  Coverage requires llvm-tools-preview component:" && echo "   If using rustup: rustup component add llvm-tools-preview" && echo "   If using Nix: See RUST-Nix.md for coverage setup instructions" && exit 1)

test-cov-open: test-cov-html
	@echo "Opening coverage report..."
	@cargo llvm-cov --workspace --html --output-dir coverage --open

install:
	@echo "Building otto..."
	@cargo build --release
	@echo ""
	@echo "Installing otto to /home/mike/bin..."
	@mkdir -p /home/mike/bin
	@cp target/release/otto /home/mike/bin
	@echo ""
	@echo "✓ otto installed successfully to /home/mike/bin/otto"

# Get current version from Cargo.toml
CURRENT_VERSION := $(shell grep -m 1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

release:
	@echo "Running tests before release..."
	@$(MAKE) test
	@echo ""
	@if [ -z "$(VERSION)" ]; then \
		echo "Error: VERSION argument is required"; \
		echo "Usage: make release VERSION=1.0.0"; \
		exit 1; \
	fi
	@if [ "$(VERSION)" != "$(CURRENT_VERSION)" ]; then \
		echo "Error: VERSION $(VERSION) does not match Cargo.toml version $(CURRENT_VERSION)"; \
		echo "Please update Cargo.toml first, or use release-patch/release-minor/release-major"; \
		exit 1; \
	fi
	@echo "Creating release tag v$(VERSION)..."
	@git tag -a "v$(VERSION)" -m "Release v$(VERSION)"
	@echo "Pushing tag to remote..."
	@git push origin "v$(VERSION)"
	@echo ""
	@echo "✓ Release v$(VERSION) tagged and pushed!"
	@echo "GitHub Actions will now build and publish the release."

release-patch:
	@echo "Running tests before release..."
	@$(MAKE) test
	@echo ""
	@echo "Current version: $(CURRENT_VERSION)"
	@NEW_VERSION=$$(echo $(CURRENT_VERSION) | awk -F. '{print $$1"."$$2"."$$3+1}'); \
	echo "Bumping to version: $$NEW_VERSION"; \
	echo "Updating Cargo.toml..."; \
	sed -i 's/^version = "$(CURRENT_VERSION)"/version = "'$$NEW_VERSION'"/' Cargo.toml && \
	echo "Committing version bump..."; \
	git add Cargo.toml && \
	git commit -m "chore: Bump version to $$NEW_VERSION" && \
	echo "Creating and pushing tag v$$NEW_VERSION..."; \
	git tag -a "v$$NEW_VERSION" -m "Release v$$NEW_VERSION" && \
	git push origin main && \
	git push origin "v$$NEW_VERSION" && \
	echo "" && \
	echo "✓ Release v$$NEW_VERSION tagged and pushed!" && \
	echo "GitHub Actions will now build and publish the release."

release-minor:
	@echo "Running tests before release..."
	@$(MAKE) test
	@echo ""
	@echo "Current version: $(CURRENT_VERSION)"
	@NEW_VERSION=$$(echo $(CURRENT_VERSION) | awk -F. '{print $$1"."$$2+1".0"}'); \
	echo "Bumping to version: $$NEW_VERSION"; \
	echo "Updating Cargo.toml..."; \
	sed -i 's/^version = "$(CURRENT_VERSION)"/version = "'$$NEW_VERSION'"/' Cargo.toml && \
	echo "Committing version bump..."; \
	git add Cargo.toml && \
	git commit -m "chore: Bump version to $$NEW_VERSION" && \
	echo "Creating and pushing tag v$$NEW_VERSION..."; \
	git tag -a "v$$NEW_VERSION" -m "Release v$$NEW_VERSION" && \
	git push origin main && \
	git push origin "v$$NEW_VERSION" && \
	echo "" && \
	echo "✓ Release v$$NEW_VERSION tagged and pushed!" && \
	echo "GitHub Actions will now build and publish the release."

release-major:
	@echo "Running tests before release..."
	@$(MAKE) test
	@echo ""
	@echo "Current version: $(CURRENT_VERSION)"
	@NEW_VERSION=$$(echo $(CURRENT_VERSION) | awk -F. '{print $$1+1".0.0"}'); \
	echo "Bumping to version: $$NEW_VERSION"; \
	echo "Updating Cargo.toml..."; \
	sed -i 's/^version = "$(CURRENT_VERSION)"/version = "'$$NEW_VERSION'"/' Cargo.toml && \
	echo "Committing version bump..."; \
	git add Cargo.toml && \
	git commit -m "chore: Bump version to $$NEW_VERSION" && \
	echo "Creating and pushing tag v$$NEW_VERSION..."; \
	git tag -a "v$$NEW_VERSION" -m "Release v$$NEW_VERSION" && \
	git push origin main && \
	git push origin "v$$NEW_VERSION" && \
	echo "" && \
	echo "✓ Release v$$NEW_VERSION tagged and pushed!" && \
	echo "GitHub Actions will now build and publish the release."

