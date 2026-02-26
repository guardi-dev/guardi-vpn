set -e
cargo build --bin p2p --release
docker compose build
docker compose up --remove-orphans