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
            host = host[1 : host.index("]")]
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

    team = config.get("team", {})
    ding = team.get("dingtalk", {})
    if team.get("enabled") is not False or ding.get("enabled") is not False:
        fail("checked-in template must remain disabled")

    public = config.get("public_base_url", "")
    if not public.startswith("https://"):
        fail("public origin must use https")

    siyuan = config.get("siyuan", {})
    if not loopback_origin(siyuan.get("base_url", "")):
        fail("SiYuan must remain internal loopback")

    compose = (root / "docker-compose.yml").read_text()
    for required in [
        "siyuan:",
        "aiks-service:",
        "image:",
        "network_mode: host",
        "env_file:",
        "- .env",
        "read_only: true",
        "cap_drop:",
        "no-new-privileges:true",
    ]:
        if required not in compose:
            fail(f"compose missing required contract: {required}")
    if "build:" in compose:
        fail("server compose must pull images, not build source")
    if "ports:" in compose or "6806:" in compose or "28081:" in compose:
        fail("compose must not publish AIKS or SiYuan ports")

    env = (root / ".env.example").read_text()
    for required in [
        "AIKS_IMAGE=",
        "SIYUAN_IMAGE=",
        "AIKS_DATA_DIR=",
        "AIKS_SIYUAN_DATA_DIR=",
    ]:
        if required not in env:
            fail(f"env missing image deployment setting: {required}")

    dockerfile = (root / "Dockerfile").read_text()
    if "cargo build --locked --release -p aiks-service" not in dockerfile:
        fail("Dockerfile build contract missing")

    entrypoint = (root / "docker-entrypoint.sh").read_text()
    if "0.0.0.0" in entrypoint or "source " in entrypoint:
        fail("docker entrypoint weakens security")

    deploy = (root / "deploy.sh").read_text()
    if "docker compose --env-file \"$ENV_FILE\"" not in deploy:
        fail("deploy.sh must pass env file explicitly")
    if "source " in deploy:
        fail("deploy.sh must not execute env")
    for action in ["init)", "check)", "pull)", "up)", "restart)", "down)", "logs)", "status)"]:
        if action not in deploy:
            fail(f"deploy.sh missing action: {action}")

    docs = (root / "SERVER_DEPLOY.md").read_text()
    for required in [
        "docker push",
        "docker compose pull",
        "network_mode: host",
        "AIKS_DINGTALK_CLIENT_SECRET",
    ]:
        if required not in docs:
            fail(f"server guide missing: {required}")

    print("team_deployment_templates_valid")


if __name__ == "__main__":
    main()
