# Potato

GUI apps, containerized. Build mini apps with a Dockerfile, distribute them with `docker push`.

Potato lets you create lightweight GUI applications packaged as OCI/Docker images. Apps are fully sandboxed, composable with shell scripts, and trivially distributable — no code signing, no platform-specific installers, no notarization workflows.

## Why?

Distributing a desktop app is painful. Electron and Tauri require code signing certificates, per-platform builds, notarization, auto-update infrastructure, and OS-specific installers. For a mini app or internal tool, that's absurd.

Potato sidesteps all of it:

- **The Dockerfile is your entire build and packaging story.** No manifests, no app store metadata.
- **`docker push` is your distribution.** OCI registries handle versioning, caching, and delivery.
- **Apps run sandboxed in containers.** No host access by default.
- **Apps are also CLI citizens.** They read stdin, write stdout, and compose with pipes like any Unix tool.

## How It Works

A Potato app is a Docker image containing:

1. **A web frontend** (HTML/JS/CSS) served as static files
2. **Backend scripts or binaries** that run inside the container

The frontend communicates with backend processes through a streaming API — no need to write your own HTTP server. Backend processes read JSON from stdin and write output to stdout. Potato handles the plumbing.

```mermaid
graph LR
    subgraph Potato
        UI["Web UI\n(frontend)"]
        API["Axum API\n/calls SSE"]
        UI --> API
    end
    subgraph Container["Docker Container"]
        HTML["/index.html"]
        CALC["/calculate.sh"]
        CHAT["/chat.sh"]
    end
    API -- "stdin/stdout" --> Container
    UI -. "static files" .-> HTML
```

## Quick Start

### 1. Create an app

Write a backend script:

```sh
#!/bin/sh
# /greet.sh — reads JSON from stdin, writes to stdout
read input
name=$(echo "$input" | jq -r '.name')
echo "{\"greeting\": \"Hello, $name!\"}"
```

Write a frontend (`index.html`):

```html
<!DOCTYPE html>
<html>
<body>
  <input id="name" placeholder="Your name" />
  <button onclick="greet()">Greet</button>
  <p id="result"></p>
  <script>
    async function greet() {
      const res = await fetch('/calls', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ cmd: ['/greet.sh'] })
      });
      const { call_id } = await res.json();

      await fetch(`/calls/${call_id}/stdin`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: { name: document.getElementById('name').value } })
      });

      const events = await fetch(`/calls/${call_id}/events`);
      const reader = events.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        for (const line of buffer.split('\n')) {
          if (!line.startsWith('data:')) continue;
          const msg = JSON.parse(line.slice(5).trim());
          if (msg.event === 'output') {
            document.getElementById('result').textContent = msg.data.greeting;
          }
        }
        buffer = '';
      }
    }
  </script>
</body>
</html>
```

Package it:

```dockerfile
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y jq && rm -rf /var/lib/apt/lists/*
COPY index.html /index.html
COPY greet.sh /greet.sh
RUN chmod +x /greet.sh
```

### 2. Build and distribute

```sh
docker build -t my-registry/greet-app .
docker push my-registry/greet-app
```

### 3. Run it

```sh
# As a GUI app
potato-gui greet-app

# From the CLI
echo '{"name": "world"}' | potato-cli greet-app /greet.sh
```

## The Streaming API

Potato exposes three endpoints that your frontend (or CLI) uses to interact with backend processes:

| Endpoint | Method | Description |
|---|---|---|
| `/calls` | POST | Create a call. Body: `{"cmd": ["/script.sh", "arg1"]}`. Returns `{"call_id": "..."}`. |
| `/calls/{id}/events` | GET | SSE stream of the process output. Each event is `{"event": "output", "data": ...}`. |
| `/calls/{id}/stdin` | POST | Send input to a running process. Body: `{"data": {...}}`. |

Backend processes can control event types by writing tagged JSON to stdout:

```sh
# Auto-tagged as {"event": "output", "data": "hello"}
echo "hello"

# Custom event type — passed through as-is
echo '{"event": "progress", "data": {"percent": 50}}'
```

Stderr output is automatically tagged as `{"event": "error", ...}`. When the process exits, an `{"event": "end"}` is sent.

## CLI Composability

Potato apps work like Unix tools:

```sh
# One-shot: run a command and get output
potato-cli my-app /calculate.sh <<< '{"a": 2, "b": 3, "op": "add"}'

# Pipe between apps
potato-cli data-loader /export.sh | potato-cli visualizer /plot.sh

# Stream interactively
echo '{"text": "hello"}' | potato-cli my-app /echo.sh
```

## Example Apps

### hello-simple

A chat app and live echo demo using vanilla HTML/JS. Demonstrates bidirectional streaming — the echo tab maintains a persistent connection where each keystroke is sent to the container and the uppercased result streams back.

### hello-world

A calculator app with a React (Vite + shadcn/ui) frontend. The frontend calls `/calculate.sh` in the container, which uses `bc` to compute results. Includes a benchmark mode.

## Project Structure

```
crates/
  potato-server/    # Axum server — calls API + static file serving
  potato-cli/       # CLI client — talks to server over Unix socket
  potato-gui/       # Tauri desktop app — web UI with fetch() polyfill
images/
  hello-simple/     # Example: vanilla HTML chat + echo app
  hello-world/      # Example: React calculator app
```

## Building

```sh
# Build all crates
cargo build

# Run the server (starts all apps)
cargo run --bin potato-server

# Run the GUI for a specific app
cargo run --bin potato-gui -- potato-hello-simple

# Run a CLI command
cargo run --bin potato-cli -- potato-hello-simple /echo.sh

# Run tests (requires Docker)
cargo test
```

## License

TBD
