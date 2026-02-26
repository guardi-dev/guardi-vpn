set -e
cargo build --bin p2p
docker compose build
docker compose up --remove-orphans