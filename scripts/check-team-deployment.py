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
}
EXPECTED_ENV = {
    "AIKS_DINGTALK_CLIENT_SECRET",
    "AIKS_SIYUAN_TOKEN",
    "AIKS_AI_API_KEY",
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
        values[key] = value
    if set(values) != EXPECTED_ENV or any(values.values()):
        fail(".env.example must contain only the expected empty secret variables")

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

    print("team_deployment_templates_valid")

if __name__ == "__main__":
    main()
