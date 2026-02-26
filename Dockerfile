# Используем официальный образ Rust
FROM rust:1.93.0
RUN apt update && apt install libpcap-dev -y
WORKDIR /app
COPY target/release/p2p ./guardi-vpn
CMD ./guardi-vpn