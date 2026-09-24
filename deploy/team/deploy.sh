#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${AIKS_DEPLOY_ENV:-$SCRIPT_DIR/.env}"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"

compose() {
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

need_env() {
  if [[ ! -f "$ENV_FILE" ]]; then
    echo "Missing $ENV_FILE. Run: ./deploy.sh init" >&2
    exit 2
  fi
}

init() {
  if [[ -e "$ENV_FILE" ]]; then
    echo "$ENV_FILE already exists; leaving it unchanged."
    return 0
  fi
  cp "$SCRIPT_DIR/.env.example" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  echo "Created $ENV_FILE. Fill real DingTalk/SiYuan values before running check/up."
}

check() {
  need_env
  compose config --quiet
  compose build aiks-service
  compose run --rm --no-deps aiks-service --check-config
}

up() {
  check
  compose up -d --build --remove-orphans
  compose ps
}

restart() {
  check
  compose up -d --build --force-recreate --remove-orphans
  compose ps
}

case "${1:-}" in
  init) init ;;
  check) check ;;
  up) up ;;
  restart) restart ;;
  down) need_env; compose down ;;
  logs) need_env; compose logs -f --tail="${2:-200}" aiks-service ;;
  status) need_env; compose ps ;;
  build) need_env; compose build aiks-service ;;
  *)
    cat >&2 <<'USAGE'
Usage: ./deploy.sh {init|check|build|up|restart|down|logs [lines]|status}

  init     Create .env from .env.example without overwriting an existing file
  check    Validate Compose, build the image, then run aiks-service --check-config
  build    Build only
  up       check + build + start
  restart  check + rebuild + force recreate
  down     Stop/remove the service container; persistent data is retained
  logs     Follow service logs (default 200 lines)
  status   Show Compose status
USAGE
    exit 2
    ;;
esac
