FROM node:24.14.0-slim AS frontend
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
RUN chmod +x /app/bin/calculate.sh
