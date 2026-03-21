FROM node:24.14.0-slim AS frontend
WORKDIR /app
RUN corepack enable pnpm
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ .
RUN pnpm build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y bc && rm -rf /var/lib/apt/lists/*
COPY --from=frontend /app/dist/ /
COPY calculate.sh /calculate.sh
RUN chmod +x /calculate.sh
