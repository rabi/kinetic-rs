# kinetic-rs justfile
# Run commands with: just <command>
# Install just: cargo install just

# Default command - show available recipes
default:
    @just --list

# Build the project
build:
    cargo build

# Build release version
release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run only unit tests (exclude integration tests)
unit-test:
    cargo test --lib

# Run only integration tests
integration-test:
    cargo test --test integration_tests

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Check code with clippy
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without modifying
fmt-check:
    cargo fmt --check

# Run all CI checks
ci: fmt-check lint test
    @echo "✅ All CI checks passed!"

# Clean build artifacts
clean:
    cargo clean

# Generate documentation
doc:
    cargo doc --no-deps --open

# Watch for changes and rebuild
watch:
    cargo watch -x build

# Watch for changes and run tests
watch-test:
    cargo watch -x test

# Run a specific example workflow
run-example example:
    cargo run -- --workflow examples/{{example}}.yaml

# Run with debug logging
run-debug workflow:
    RUST_LOG=debug cargo run -- --workflow {{workflow}}

# Check for outdated dependencies
outdated:
    cargo outdated

# Update dependencies
update:
    cargo update

# Show dependency tree
deps:
    cargo tree

# Run security audit
audit:
    cargo audit

# Create a new release
tag version:
    git tag -a v{{version}} -m "Release v{{version}}"
    git push origin v{{version}}
