FROM node:24.15.0-slim AS frontend
WORKDIR /build
RUN corepack enable pnpm
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ .
RUN pnpm build

FROM spudkit-base
RUN apt-get update && apt-get install -y bc && rm -rf /var/lib/apt/lists/*
COPY --from=frontend /build/dist/ /app/gui/
COPY calculate.sh /app/bin/calculate.sh
COPY calculate-loop.sh /app/bin/calculate-loop.sh
COPY noop-loop.sh /app/bin/noop-loop.sh
RUN chmod +x /app/bin/calculate.sh /app/bin/calculate-loop.sh /app/bin/noop-loop.sh
