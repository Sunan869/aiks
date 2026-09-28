#!/bin/sh
set -eu

fail() {
  printf 'AIKS collector configuration error: %s\n' "$1" >&2
  exit 2
}

require_env() {
  name="$1"
  value="$(printenv "$name" 2>/dev/null || true)"
  [ -n "$value" ] || fail "$name is required"
}

toml_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

AIKS_COLLECTOR_PORT="${AIKS_COLLECTOR_PORT:-28082}"
AIKS_CONFIG_PATH="${AIKS_CONFIG_PATH:-/run/aiks/service.toml}"
AIKS_DATA_DIR="${AIKS_DATA_DIR:-/var/lib/aiks-collector}"
AIKS_WEKNORA_CHANNEL="${AIKS_WEKNORA_CHANNEL:-aiks}"

for name in AIKS_COLLECTOR_TOKEN AIKS_WEKNORA_BASE_URL AIKS_WEKNORA_API_KEY; do
  require_env "$name"
done

case "$AIKS_COLLECTOR_PORT" in
  ''|*[!0-9]*) fail "AIKS_COLLECTOR_PORT must be an integer" ;;
esac
[ "$AIKS_COLLECTOR_PORT" -gt 0 ] && [ "$AIKS_COLLECTOR_PORT" -le 65535 ] \
  || fail "AIKS_COLLECTOR_PORT must be between 1 and 65535"

if ! printf '%s' "$AIKS_COLLECTOR_TOKEN" | grep -Eq '^[0-9A-Fa-f]{64}$'; then
  fail "AIKS_COLLECTOR_TOKEN must be exactly 64 hexadecimal characters"
fi

case "$AIKS_WEKNORA_CHANNEL" in
  ''|*[!A-Za-z0-9_-]*) fail "AIKS_WEKNORA_CHANNEL contains unsupported characters" ;;
esac

mkdir -p "$(dirname "$AIKS_CONFIG_PATH")" "$AIKS_DATA_DIR"
cat > "$AIKS_CONFIG_PATH" <<EOF_CONFIG
mode = "collector"
listen = "0.0.0.0:${AIKS_COLLECTOR_PORT}"
database = "${AIKS_DATA_DIR}/state.db"

[collector]
token_env = "AIKS_COLLECTOR_TOKEN"

[weknora]
enabled = true
base_url = "$(toml_escape "$AIKS_WEKNORA_BASE_URL")"
knowledge_base_id = ""
api_key_env = "AIKS_WEKNORA_API_KEY"
channel = "$(toml_escape "$AIKS_WEKNORA_CHANNEL")"
dynamic_targets = true
EOF_CONFIG
chmod 0600 "$AIKS_CONFIG_PATH"

exec /usr/local/bin/aiks-service --config "$AIKS_CONFIG_PATH" "$@"
