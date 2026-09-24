#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${AIKS_DEPLOY_ENV:-$SCRIPT_DIR/.env}"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"

compose() {
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

need_env() {
  [[ -f "$ENV_FILE" ]] || { echo "Missing $ENV_FILE. Run: ./deploy.sh init" >&2; exit 2; }
}

init() {
  [[ -e "$ENV_FILE" ]] && { echo "$ENV_FILE already exists"; return; }
  cp "$SCRIPT_DIR/.env.example" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
}

check() {
  need_env
  compose config --quiet
}

pull() {
  need_env
  compose pull
}

up() {
  check
  pull
  compose up -d --remove-orphans
  compose ps
}

restart() {
  check
  pull
  compose up -d --force-recreate --remove-orphans
  compose ps
}

case "${1:-}" in
  init) init ;;
  check) check ;;
  pull) pull ;;
  up) up ;;
  restart) restart ;;
  down) need_env; compose down ;;
  logs) need_env; compose logs -f --tail="${2:-200}" aiks-service siyuan ;;
  status) need_env; compose ps ;;
  *)
    echo "Usage: ./deploy.sh {init|check|pull|up|restart|down|logs [lines]|status}" >&2
    exit 2
    ;;
esac
