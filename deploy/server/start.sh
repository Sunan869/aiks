#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

COMPOSE=(docker compose --env-file .env -f docker-compose.yml)

die() {
  echo "[ERROR] $*" >&2
  exit 1
}

env_value() {
  local key="$1" line
  line="$(grep -m1 -E "^${key}=" .env 2>/dev/null || true)"
  printf '%s' "${line#*=}"
}

set_env() {
  local key="$1" value="$2"
  if grep -qE "^${key}=" .env; then
    sed -i "s|^${key}=.*|${key}=${value}|" .env
  else
    printf '%s=%s\n' "$key" "$value" >> .env
  fi
}

random_hex_32() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    od -An -N32 -tx1 /dev/urandom | tr -d ' \n'
  fi
}

load_images() {
  mkdir -p images
  local found=0 file
  shopt -s nullglob
  for file in images/*.tar; do
    found=1
    echo "[INFO] docker load: $file"
    docker load -i "$file"
  done
  for file in images/*.tar.gz images/*.tgz; do
    found=1
    echo "[INFO] docker load: $file"
    gzip -dc "$file" | docker load
  done
  shopt -u nullglob
  if [ "$found" -eq 0 ]; then
    echo "[INFO] images/ has no image archive; using already loaded local image."
  fi
}

ensure_env() {
  if [ ! -f .env ]; then
    cp .env.example .env
    echo "[INFO] created .env from .env.example"
  fi
  if [ -z "$(env_value AIKS_COLLECTOR_TOKEN)" ]; then
    set_env AIKS_COLLECTOR_TOKEN "$(random_hex_32)"
    echo "[INFO] generated AIKS_COLLECTOR_TOKEN"
  fi
  [ -n "$(env_value AIKS_WEKNORA_API_KEY)" ] || die "AIKS_WEKNORA_API_KEY is empty. Start WeKnora first, create the server platform API key, fill .env, then rerun ./start.sh."
}

ensure_network() {
  local network
  network="$(env_value AIKS_TEAM_NETWORK)"
  network="${network:-aiks-team-network}"
  if ! docker network inspect "$network" >/dev/null 2>&1; then
    docker network create "$network" >/dev/null
    echo "[INFO] created docker network: $network"
  fi
}

start_service() {
  ensure_env
  load_images
  ensure_network
  local image data_dir
  image="$(env_value AIKS_COLLECTOR_IMAGE)"
  image="${image:-aiks-service:weknora-collector}"
  data_dir="$(env_value AIKS_COLLECTOR_DATA_DIR)"
  data_dir="${data_dir:-./data}"
  docker image inspect "$image" >/dev/null 2>&1 \
    || die "missing image: $image. Put the tar under images/ or docker load it first."

  mkdir -p "$data_dir"
  "${COMPOSE[@]}" config >/dev/null
  "${COMPOSE[@]}" up -d --force-recreate --remove-orphans

  local container
  container="$(env_value AIKS_COLLECTOR_CONTAINER_NAME)"
  container="${container:-aiks-collector}"
  echo "[INFO] waiting for collector health..."
  for _ in $(seq 1 30); do
    if docker exec "$container" curl -fsS http://127.0.0.1:28082/healthz >/dev/null 2>&1; then
      echo "[OK] AIKS collector is healthy."
      "${COMPOSE[@]}" ps
      return 0
    fi
    sleep 2
  done
  "${COMPOSE[@]}" ps
  docker logs --tail 120 "$container" || true
  die "collector health check timed out"
}

case "${1:-start}" in
  start|up) start_service ;;
  restart)
    "${COMPOSE[@]}" down
    start_service
    ;;
  stop|down) "${COMPOSE[@]}" down ;;
  status|ps) "${COMPOSE[@]}" ps ;;
  logs) "${COMPOSE[@]}" logs -f --tail=200 ;;
  *)
    echo "Usage: $0 {start|restart|stop|status|logs}" >&2
    exit 2
    ;;
esac
