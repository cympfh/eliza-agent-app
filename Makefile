.PHONY: help build-windows build-windows-release clean setup-windows-target install-deps

help:
	@echo "Available targets:"
	@echo "  install-deps            - Install required system dependencies (mingw-w64)"
	@echo "  setup-windows-target    - Install Windows cross-compilation target"
	@echo "  build-windows           - Build debug exe for Windows"
	@echo "  build-windows-release   - Build release exe for Windows"
	@echo "  clean                   - Clean build artifacts"

# Install system dependencies for Windows cross-compilation
install-deps:
	@echo "Installing mingw-w64 toolchain..."
	sudo apt update
	sudo apt install -y mingw-w64

# Install Windows target for cross-compilation
setup-windows-target:
	rustup target add x86_64-pc-windows-gnu

# Build debug Windows executable
build-windows: setup-windows-target
	cargo build --target x86_64-pc-windows-gnu
	@echo ""
	@echo "Debug executable created at: target/x86_64-pc-windows-gnu/debug/talk-with-grok.exe"

# Build release Windows executable
build-windows-release: setup-windows-target
	cargo build --release --target x86_64-pc-windows-gnu
	@echo ""
	@echo "Release executable created at: target/x86_64-pc-windows-gnu/release/talk-with-grok.exe"

# Clean build artifacts
clean:
	cargo clean
