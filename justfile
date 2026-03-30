# Build the GUI polyfill (TypeScript → IIFE)
polyfill:
    cd crates/spudkit-gui && pnpm install && pnpm build

# Build all Rust crates (builds polyfill first)
build: polyfill
    cargo build

# Run unit and integration tests (requires Docker)
test:
    cargo test

# Build all Docker images
images:
    docker build -t spud-hello-world images/hello-world
    docker build -t spud-hello-simple images/hello-simple
    docker build -t spud-book-search images/book-search

# Build the hello-world frontend (requires pnpm)
frontend:
    cd images/hello-world/frontend && pnpm build

# Install all binaries locally
install:
    cargo install --path crates/spudkit
    cargo install --path crates/spudkit-cli
    cargo install --path crates/spudkit-gui
    cargo install --path crates/spud-app-chromium

# Run clippy
lint:
    cargo clippy -- -D warnings

# Type-check the GUI TypeScript
typecheck:
    cd crates/spudkit-gui && pnpm typecheck

# Run all pre-commit checks
check:
    prek run --all-files

# Run e2e tests for hello-world app
e2e:
    cd e2e && pnpm test

# Run e2e tests for book-search app
e2e-book-search:
    cd e2e && pnpm test:book-search

# Run all e2e tests
e2e-all:
    cd e2e && pnpm test
    cd e2e && pnpm test:book-search

# Full build + test cycle
all: build images test e2e-all

# Start the server
server:
    cargo run -p spudkit

# Open an app in the GUI (usage: just app hello-world)
app name:
    WEBKIT_DISABLE_DMABUF_RENDERER=1 cargo run -p spudkit-gui -- {{name}}

# Run a CLI command (usage: just cli hello-simple echo.sh)
cli name cmd:
    cargo run -p spudkit-cli -- run {{name}} {{cmd}}

# List available spuds
ls:
    cargo run -p spudkit-cli -- ls
