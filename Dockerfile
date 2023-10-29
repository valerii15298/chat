FROM rust:latest as backend-builder
COPY ./server /usr/src/app
WORKDIR /usr/src/app
RUN --mount=type=cache,target=/usr/local/cargo,from=rust:latest,source=/usr/local/cargo \
    --mount=type=cache,target=target \
    cargo build --release && mv ./target/release/chat-server ./chat-server


FROM node:lts AS frontend-builder
RUN npm -g add pnpm
COPY ./client /usr/src/app
WORKDIR /usr/src/app
RUN pnpm install && pnpm build


# Runtime image
FROM debian:bookworm-slim
# Run as "app" user
RUN useradd -ms /bin/bash app
USER app
WORKDIR /app
# Get compiled binaries from builder's cargo install directory
COPY --from=backend-builder /usr/src/app/chat-server /app/chat-server
COPY --from=frontend-builder /usr/src/app/dist /app/dist
# Run the app
CMD ./chat-server
