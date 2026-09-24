#!/bin/sh
set -eu

fail() {
  printf 'AIKS docker configuration error: %s\n' "$1" >&2
  exit 2
}

require_env() {
  name="$1"
  value="$(printenv "$name" 2>/dev/null || true)"
  [ -n "$value" ] || fail "$name is required"
}

validate_bool() {
  case "$2" in
    true|false) ;;
    *) fail "$1 must be true or false" ;;
  esac
}

validate_uint() {
  case "$2" in
    ''|*[!0-9]*) fail "$1 must be an unsigned integer" ;;
  esac
}

validate_text() {
  name="$1"
  value="$2"
  if printf '%s' "$value" | LC_ALL=C grep -q '[[:cntrl:]]'; then
    fail "$name must not contain control characters"
  fi
}

toml_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

AIKS_SERVICE_PORT="${AIKS_SERVICE_PORT:-28081}"
AIKS_DIRECTORY_REFRESH_SECONDS="${AIKS_DIRECTORY_REFRESH_SECONDS:-300}"
AIKS_DIRECTORY_MAX_STALE_SECONDS="${AIKS_DIRECTORY_MAX_STALE_SECONDS:-900}"
AIKS_LOGIN_ATTEMPT_TTL_SECONDS="${AIKS_LOGIN_ATTEMPT_TTL_SECONDS:-300}"
AIKS_ACCESS_TOKEN_TTL_SECONDS="${AIKS_ACCESS_TOKEN_TTL_SECONDS:-900}"
AIKS_REFRESH_TOKEN_TTL_SECONDS="${AIKS_REFRESH_TOKEN_TTL_SECONDS:-604800}"
AIKS_AI_ENABLED="${AIKS_AI_ENABLED:-false}"
AIKS_AI_TIMEOUT_SECONDS="${AIKS_AI_TIMEOUT_SECONDS:-120}"
AIKS_AI_MAX_CONCURRENT="${AIKS_AI_MAX_CONCURRENT:-1}"
AIKS_EMBEDDING_ENABLED="${AIKS_EMBEDDING_ENABLED:-false}"
AIKS_EMBEDDING_BATCH_SIZE="${AIKS_EMBEDDING_BATCH_SIZE:-16}"
AIKS_EMBEDDING_CHUNK_TARGET_TOKENS="${AIKS_EMBEDDING_CHUNK_TARGET_TOKENS:-800}"
AIKS_EMBEDDING_CHUNK_MAX_TOKENS="${AIKS_EMBEDDING_CHUNK_MAX_TOKENS:-1200}"
AIKS_EMBEDDING_CHUNK_OVERLAP_TOKENS="${AIKS_EMBEDDING_CHUNK_OVERLAP_TOKENS:-120}"
AIKS_SIYUAN_BASE_URL="${AIKS_SIYUAN_BASE_URL:-http://127.0.0.1:6806}"
AIKS_SIYUAN_NOTEBOOK_NAME="${AIKS_SIYUAN_NOTEBOOK_NAME:-AIKS}"
AIKS_SIYUAN_SESSION_NOTEBOOK_NAME="${AIKS_SIYUAN_SESSION_NOTEBOOK_NAME:-AIKS}"
AIKS_SIYUAN_SESSION_ROOT="${AIKS_SIYUAN_SESSION_ROOT:-/10 AI Sessions}"
AIKS_SIYUAN_KNOWLEDGE_ROOT="${AIKS_SIYUAN_KNOWLEDGE_ROOT:-/20 Knowledge}"
AIKS_CONFIG_PATH="${AIKS_CONFIG_PATH:-/run/aiks/service.toml}"

for name in \
  AIKS_PUBLIC_HOST AIKS_PUBLIC_BASE_URL \
  AIKS_DINGTALK_CORP_ID AIKS_DINGTALK_CLIENT_ID \
  AIKS_DINGTALK_CLIENT_SECRET AIKS_DINGTALK_ROOT_DEPARTMENT_IDS \
  AIKS_SIYUAN_TOKEN; do
  require_env "$name"
done

[ "$AIKS_PUBLIC_BASE_URL" = "https://${AIKS_PUBLIC_HOST}" ] \
  || fail "AIKS_PUBLIC_BASE_URL must equal https://AIKS_PUBLIC_HOST"
case "$AIKS_PUBLIC_HOST" in
  *'/'*|*'?'*|*'#'*|*'@'*) fail "AIKS_PUBLIC_HOST must be an authority only" ;;
esac
[ "$AIKS_SIYUAN_BASE_URL" = "http://127.0.0.1:6806" ] \
  || fail "AIKS_SIYUAN_BASE_URL must remain http://127.0.0.1:6806 in the current team contract"

for pair in \
  "AIKS_SERVICE_PORT:$AIKS_SERVICE_PORT" \
  "AIKS_DIRECTORY_REFRESH_SECONDS:$AIKS_DIRECTORY_REFRESH_SECONDS" \
  "AIKS_DIRECTORY_MAX_STALE_SECONDS:$AIKS_DIRECTORY_MAX_STALE_SECONDS" \
  "AIKS_LOGIN_ATTEMPT_TTL_SECONDS:$AIKS_LOGIN_ATTEMPT_TTL_SECONDS" \
  "AIKS_ACCESS_TOKEN_TTL_SECONDS:$AIKS_ACCESS_TOKEN_TTL_SECONDS" \
  "AIKS_REFRESH_TOKEN_TTL_SECONDS:$AIKS_REFRESH_TOKEN_TTL_SECONDS" \
  "AIKS_AI_TIMEOUT_SECONDS:$AIKS_AI_TIMEOUT_SECONDS" \
  "AIKS_AI_MAX_CONCURRENT:$AIKS_AI_MAX_CONCURRENT" \
  "AIKS_EMBEDDING_BATCH_SIZE:$AIKS_EMBEDDING_BATCH_SIZE" \
  "AIKS_EMBEDDING_CHUNK_TARGET_TOKENS:$AIKS_EMBEDDING_CHUNK_TARGET_TOKENS" \
  "AIKS_EMBEDDING_CHUNK_MAX_TOKENS:$AIKS_EMBEDDING_CHUNK_MAX_TOKENS" \
  "AIKS_EMBEDDING_CHUNK_OVERLAP_TOKENS:$AIKS_EMBEDDING_CHUNK_OVERLAP_TOKENS"; do
  name=${pair%%:*}
  value=${pair#*:}
  validate_uint "$name" "$value"
done

[ "$AIKS_SERVICE_PORT" -gt 0 ] && [ "$AIKS_SERVICE_PORT" -le 65535 ] \
  || fail "AIKS_SERVICE_PORT must be between 1 and 65535"
validate_bool AIKS_AI_ENABLED "$AIKS_AI_ENABLED"
validate_bool AIKS_EMBEDDING_ENABLED "$AIKS_EMBEDDING_ENABLED"

validate_text AIKS_PUBLIC_HOST "$AIKS_PUBLIC_HOST"
validate_text AIKS_PUBLIC_BASE_URL "$AIKS_PUBLIC_BASE_URL"
validate_text AIKS_DINGTALK_CORP_ID "$AIKS_DINGTALK_CORP_ID"
validate_text AIKS_DINGTALK_CLIENT_ID "$AIKS_DINGTALK_CLIENT_ID"
validate_text AIKS_AI_BASE_URL "${AIKS_AI_BASE_URL:-}"
validate_text AIKS_AI_MODEL "${AIKS_AI_MODEL:-}"
validate_text AIKS_EMBEDDING_BASE_URL "${AIKS_EMBEDDING_BASE_URL:-}"
validate_text AIKS_EMBEDDING_MODEL "${AIKS_EMBEDDING_MODEL:-}"
validate_text AIKS_SIYUAN_BASE_URL "$AIKS_SIYUAN_BASE_URL"
validate_text AIKS_SIYUAN_NOTEBOOK_NAME "$AIKS_SIYUAN_NOTEBOOK_NAME"
validate_text AIKS_SIYUAN_SESSION_NOTEBOOK_NAME "$AIKS_SIYUAN_SESSION_NOTEBOOK_NAME"
validate_text AIKS_SIYUAN_SESSION_ROOT "$AIKS_SIYUAN_SESSION_ROOT"
validate_text AIKS_SIYUAN_KNOWLEDGE_ROOT "$AIKS_SIYUAN_KNOWLEDGE_ROOT"

if [ "$AIKS_AI_ENABLED" = true ]; then
  require_env AIKS_AI_BASE_URL
  require_env AIKS_AI_MODEL
fi
if [ "$AIKS_EMBEDDING_ENABLED" = true ]; then
  require_env AIKS_EMBEDDING_BASE_URL
  require_env AIKS_EMBEDDING_MODEL
fi

roots="$AIKS_DINGTALK_ROOT_DEPARTMENT_IDS"
if ! root_toml="$(printf '%s\n' "$roots" | awk -F, '
  NF < 1 || NF > 100 { exit 1 }
  {
    for (i = 1; i <= NF; i++) {
      if ($i !~ /^[1-9][0-9]*$/ || length($i) > 18) exit 1
      if (i > 1) printf ", "
      printf "\\\"%s\\\"", $i
    }
  }
')"; then
  fail "AIKS_DINGTALK_ROOT_DEPARTMENT_IDS must be 1-100 comma-separated positive integers"
fi

ai_key_env=''
[ -z "${AIKS_AI_API_KEY:-}" ] || ai_key_env='AIKS_AI_API_KEY'
embedding_key_env=''
[ -z "${AIKS_EMBEDDING_API_KEY:-}" ] || embedding_key_env='AIKS_EMBEDDING_API_KEY'

mkdir -p "$(dirname "$AIKS_CONFIG_PATH")"
cat > "$AIKS_CONFIG_PATH" <<EOF_CONFIG
mode = "team"
listen = "127.0.0.1:${AIKS_SERVICE_PORT}"
database = "/var/lib/aiks-team/business/state.db"

[team]
enabled = true
public_base_url = "$(toml_escape "$AIKS_PUBLIC_BASE_URL")"

[team.dingtalk]
enabled = true
corp_id = "$(toml_escape "$AIKS_DINGTALK_CORP_ID")"
client_id = "$(toml_escape "$AIKS_DINGTALK_CLIENT_ID")"
redirect_uri = "$(toml_escape "$AIKS_PUBLIC_BASE_URL")/api/v1/auth/dingtalk/callback"
client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET"
client_secret_file = ""

[team.directory]
root_department_ids = [${root_toml}]
refresh_interval_seconds = ${AIKS_DIRECTORY_REFRESH_SECONDS}
max_stale_seconds = ${AIKS_DIRECTORY_MAX_STALE_SECONDS}

[team.sessions]
login_attempt_ttl_seconds = ${AIKS_LOGIN_ATTEMPT_TTL_SECONDS}
access_token_ttl_seconds = ${AIKS_ACCESS_TOKEN_TTL_SECONDS}
refresh_token_ttl_seconds = ${AIKS_REFRESH_TOKEN_TTL_SECONDS}

[ai]
enabled = ${AIKS_AI_ENABLED}
base_url = "$(toml_escape "${AIKS_AI_BASE_URL:-}")"
model = "$(toml_escape "${AIKS_AI_MODEL:-}")"
timeout_seconds = ${AIKS_AI_TIMEOUT_SECONDS}
max_concurrent = ${AIKS_AI_MAX_CONCURRENT}

[embedding]
enabled = ${AIKS_EMBEDDING_ENABLED}
base_url = "$(toml_escape "${AIKS_EMBEDDING_BASE_URL:-}")"
model = "$(toml_escape "${AIKS_EMBEDDING_MODEL:-}")"
batch_size = ${AIKS_EMBEDDING_BATCH_SIZE}
chunk_target_tokens = ${AIKS_EMBEDDING_CHUNK_TARGET_TOKENS}
chunk_max_tokens = ${AIKS_EMBEDDING_CHUNK_MAX_TOKENS}
chunk_overlap_tokens = ${AIKS_EMBEDDING_CHUNK_OVERLAP_TOKENS}

[model_credentials]
ai_api_key_env = "${ai_key_env}"
embedding_api_key_env = "${embedding_key_env}"

[siyuan]
base_url = "$(toml_escape "$AIKS_SIYUAN_BASE_URL")"
token = ""
token_env = "AIKS_SIYUAN_TOKEN"
notebook_name = "$(toml_escape "$AIKS_SIYUAN_NOTEBOOK_NAME")"
session_notebook_name = "$(toml_escape "$AIKS_SIYUAN_SESSION_NOTEBOOK_NAME")"
session_root = "$(toml_escape "$AIKS_SIYUAN_SESSION_ROOT")"
knowledge_root = "$(toml_escape "$AIKS_SIYUAN_KNOWLEDGE_ROOT")"
EOF_CONFIG
chmod 0600 "$AIKS_CONFIG_PATH"

checking=false
for arg in "$@"; do
  [ "$arg" = "--check-config" ] && checking=true
done
if [ "$checking" = false ]; then
  mkdir -p /var/lib/aiks-team/business
fi

exec /usr/local/bin/aiks-service --config "$AIKS_CONFIG_PATH" "$@"
