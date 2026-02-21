set -e
cargo build
docker compose build
docker compose up --remove-orphans