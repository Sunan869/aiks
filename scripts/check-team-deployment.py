#!/usr/bin/env python3
import ipaddress
import pathlib
import sys
import tomllib

REQUIRED = {
    "service.toml.example",
    ".env.example",
    "nginx.conf.example",
    "aiks-service.service.example",
    "README.md",
    "SERVER_DEPLOY.md",
    "Dockerfile",
    "docker-compose.yml",
    "docker-entrypoint.sh",
    "deploy.sh",
}
EXPECTED_ENV = {
    "AIKS_RUST_VERSION",
    "AIKS_IMAGE_NAME",
    "AIKS_IMAGE_TAG",
    "AIKS_CONTAINER_NAME",
    "AIKS_DATA_DIR",
    "AIKS_SERVICE_PORT",
    "AIKS_PUBLIC_HOST",
    "AIKS_PUBLIC_BASE_URL",
    "AIKS_DINGTALK_CORP_ID",
    "AIKS_DINGTALK_CLIENT_ID",
    "AIKS_DINGTALK_CLIENT_SECRET",
    "AIKS_DINGTALK_ROOT_DEPARTMENT_IDS",
    "AIKS_DIRECTORY_REFRESH_SECONDS",
    "AIKS_DIRECTORY_MAX_STALE_SECONDS",
    "AIKS_LOGIN_ATTEMPT_TTL_SECONDS",
    "AIKS_ACCESS_TOKEN_TTL_SECONDS",
    "AIKS_REFRESH_TOKEN_TTL_SECONDS",
    "AIKS_SIYUAN_BASE_URL",
    "AIKS_SIYUAN_TOKEN",
    "AIKS_SIYUAN_NOTEBOOK_NAME",
    "AIKS_SIYUAN_SESSION_NOTEBOOK_NAME",
    "AIKS_SIYUAN_SESSION_ROOT",
    "AIKS_SIYUAN_KNOWLEDGE_ROOT",
    "AIKS_AI_ENABLED",
    "AIKS_AI_BASE_URL",
    "AIKS_AI_MODEL",
    "AIKS_AI_API_KEY",
    "AIKS_AI_TIMEOUT_SECONDS",
    "AIKS_AI_MAX_CONCURRENT",
    "AIKS_EMBEDDING_ENABLED",
    "AIKS_EMBEDDING_BASE_URL",
    "AIKS_EMBEDDING_MODEL",
    "AIKS_EMBEDDING_API_KEY",
    "AIKS_EMBEDDING_BATCH_SIZE",
    "AIKS_EMBEDDING_CHUNK_TARGET_TOKENS",
    "AIKS_EMBEDDING_CHUNK_MAX_TOKENS",
    "AIKS_EMBEDDING_CHUNK_OVERLAP_TOKENS",
}
EMPTY_EXAMPLE_VALUES = {
    "AIKS_DINGTALK_CORP_ID",
    "AIKS_DINGTALK_CLIENT_ID",
    "AIKS_DINGTALK_CLIENT_SECRET",
    "AIKS_SIYUAN_TOKEN",
    "AIKS_AI_BASE_URL",
    "AIKS_AI_MODEL",
    "AIKS_AI_API_KEY",
    "AIKS_EMBEDDING_BASE_URL",
    "AIKS_EMBEDDING_MODEL",
    "AIKS_EMBEDDING_API_KEY",
}

def fail(message: str) -> None:
    raise SystemExit(f"team deployment check failed: {message}")

def loopback_origin(value: str) -> bool:
    try:
        scheme, rest = value.split("://", 1)
        hostport = rest.rstrip("/")
        if "/" in hostport or "@" in hostport or "?" in hostport or "#" in hostport:
            return False
        host = hostport
        if host.startswith("["):
            host = host[1:host.index("]")]
        elif ":" in host:
            host = host.rsplit(":", 1)[0]
        return scheme == "http" and ipaddress.ip_address(host).is_loopback
    except Exception:
        return False

def main() -> None:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "deploy/team")
    missing = sorted(name for name in REQUIRED if not (root / name).is_file())
    if missing:
        fail("missing: " + ", ".join(missing))

    config = tomllib.loads((root / "service.toml.example").read_text())
    if config.get("mode") != "team":
        fail("template mode must be team")
    listen = config.get("listen", "")
    try:
        host, port = listen.rsplit(":", 1)
        if not ipaddress.ip_address(host.strip("[]")).is_loopback or int(port) <= 0:
            fail("listen must be numeric loopback with a fixed port")
    except Exception:
        fail("invalid listen address")
    if not pathlib.PurePosixPath(config.get("database", "")).is_absolute():
        fail("database path must be absolute")

    team = config.get("team", {})
    ding = team.get("dingtalk", {})
    if team.get("enabled") is not False or ding.get("enabled") is not False:
        fail("checked-in template must remain disabled")
    public = team.get("public_base_url", "")
    callback = ding.get("redirect_uri", "")
    if not public.startswith("https://") or callback != public + "/api/v1/auth/dingtalk/callback":
        fail("public origin/callback mismatch")
    if ding.get("client_secret_env") != "AIKS_DINGTALK_CLIENT_SECRET" or ding.get("client_secret_file"):
        fail("template secret reference drifted")

    siyuan = config.get("siyuan", {})
    if not loopback_origin(siyuan.get("base_url", "")):
        fail("SiYuan must remain on an internal numeric loopback origin")
    if siyuan.get("token"):
        fail("inline SiYuan token is forbidden in deployment template")
    if siyuan.get("token_env") != "AIKS_SIYUAN_TOKEN":
        fail("SiYuan token reference drifted")

    values = {}
    for raw in (root / ".env.example").read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            fail("invalid .env.example line")
        key, value = line.split("=", 1)
        if not key or key in values:
            fail("invalid or duplicate .env.example key")
        values[key] = value
    if set(values) != EXPECTED_ENV:
        fail(".env.example keys drifted")
    if any(values[key] for key in EMPTY_EXAMPLE_VALUES):
        fail(".env.example contains a credential or environment-specific identity")
    if values["AIKS_PUBLIC_BASE_URL"] != "https://" + values["AIKS_PUBLIC_HOST"]:
        fail(".env.example public host/base mismatch")
    if values["AIKS_SIYUAN_BASE_URL"] != "http://127.0.0.1:6806":
        fail("Docker SiYuan origin must stay on host loopback")
    if values["AIKS_AI_ENABLED"] != "false" or values["AIKS_EMBEDDING_ENABLED"] != "false":
        fail("models must remain disabled in the checked-in example")

    nginx = (root / "nginx.conf.example").read_text()
    for required in [
        "listen 443 ssl",
        "proxy_pass http://127.0.0.1:28081",
        "proxy_set_header Host aiks.example.com",
        "location / {",
        "return 404",
    ]:
        if required not in nginx:
            fail(f"nginx missing required contract: {required}")
    for forbidden in [
        "127.0.0.1:6806",
        "/proxy/",
        "/api/file/",
        "/api/query/sql",
        "$proxy_add_x_forwarded_for",
    ]:
        if forbidden in nginx:
            fail(f"nginx exposes forbidden surface: {forbidden}")
    for name in ["Forwarded", "X-Forwarded-For", "X-Forwarded-Host", "X-Forwarded-Proto"]:
        if f'proxy_set_header {name} "";' not in nginx:
            fail(f"nginx must clear {name}")

    unit = (root / "aiks-service.service.example").read_text()
    if "EnvironmentFile=/etc/aiks/team.env" not in unit:
        fail("systemd unit must inject secrets through EnvironmentFile")
    if "aiks-service --config /etc/aiks/service.toml" not in unit or "--bootstrap-stdin" in unit:
        fail("systemd team startup command is invalid")
    if "UMask=0077" not in unit or "NoNewPrivileges=true" not in unit:
        fail("systemd hardening contract missing")

    compose = (root / "docker-compose.yml").read_text()
    for required in [
        "network_mode: host",
        "env_file:",
        "- .env",
        "read_only: true",
        "cap_drop:",
        "no-new-privileges:true",
        "AIKS_CONFIG_PATH: /run/aiks/service.toml",
    ]:
        if required not in compose:
            fail(f"compose missing required contract: {required}")
    if "ports:" in compose or "127.0.0.1:6806:" in compose:
        fail("compose must not publish AIKS or SiYuan ports")

    dockerfile = (root / "Dockerfile").read_text()
    if "cargo build --locked --release -p aiks-service" not in dockerfile:
        fail("Dockerfile must build the locked aiks-service package")
    if 'ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]' not in dockerfile:
        fail("Dockerfile entrypoint contract missing")
    if "EXPOSE" in dockerfile:
        fail("Dockerfile must not imply a public team-service port")

    entrypoint = (root / "docker-entrypoint.sh").read_text()
    for required in [
        'listen = "127.0.0.1:${AIKS_SERVICE_PORT}"',
        'client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET"',
        'token_env = "AIKS_SIYUAN_TOKEN"',
        'database = "/var/lib/aiks-team/business/state.db"',
    ]:
        if required not in entrypoint:
            fail(f"docker entrypoint missing required contract: {required}")
    if "0.0.0.0" in entrypoint or "source " in entrypoint:
        fail("docker entrypoint weakens listener or executes the env file")

    deploy = (root / "deploy.sh").read_text()
    if 'docker compose --env-file "$ENV_FILE"' not in deploy:
        fail("deploy.sh must pass .env to Compose without sourcing it")
    if "source " in deploy or "Invoke-Expression" in deploy:
        fail("deploy.sh must not execute .env contents")
    for action in ["init)", "check)", "up)", "restart)", "down)", "logs)", "status)"]:
        if action not in deploy:
            fail(f"deploy.sh missing action: {action}")

    docs = (root / "SERVER_DEPLOY.md").read_text()
    for required in [
        "./deploy.sh check",
        "network_mode: host",
        "127.0.0.1:6806",
        "AIKS_DINGTALK_CLIENT_SECRET",
    ]:
        if required not in docs:
            fail(f"server deployment guide missing: {required}")

    print("team_deployment_templates_valid")

if __name__ == "__main__":
    main()
