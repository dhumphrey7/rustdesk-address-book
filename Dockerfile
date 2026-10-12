# Stage 1: Build frontend
FROM node:22-alpine AS frontend
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npx vite build

# Stage 2: Build Rust backend
FROM rust:1.89-bookworm AS backend
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY migrations/ migrations/
COPY --from=frontend /app/web/dist/ web/dist/
RUN cargo build --release

# Stage 3: Minimal runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /app/target/release/rustdesk-address-book .
COPY --from=backend /app/migrations/ migrations/
EXPOSE 21114
ENV RUSTDESK_AB_DB_PATH=/data/db.sqlite3
VOLUME /data
CMD ["./rustdesk-address-book"]
