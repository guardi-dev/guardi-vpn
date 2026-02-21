# Используем официальный образ Rust
FROM rust:1.93.0
WORKDIR /app
COPY target/debug .
CMD ./guardi-vpn