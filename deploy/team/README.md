# AIKS single-company team deployment

本目录提供 S2 单企业团队服务的两种部署入口：

- **Docker Compose（推荐用于当前服务器联调）**：`docker-compose.yml` + `.env` + `deploy.sh`；
- **systemd + 宿主机 Nginx**：`service.toml.example` + `aiks-service.service.example` + `nginx.conf.example`。

完整 Docker 部署步骤见 [`SERVER_DEPLOY.md`](./SERVER_DEPLOY.md)。

## Docker 快速开始

```bash
cd deploy/team
chmod +x deploy.sh
./deploy.sh init
vim .env
./deploy.sh check
./deploy.sh up
./deploy.sh logs
```

Compose 使用 `network_mode: host`，但 AIKS Service 本身仍只监听 `127.0.0.1:${AIKS_SERVICE_PORT}`，没有 `ports:` 公网映射。这是当前 team 安全契约的一部分：宿主机 Nginx 负责 TLS/公网入口，SiYuan 只在 `127.0.0.1:6806` 内部可达。

`.env` 中的 DingTalk Client Secret、SiYuan Token 和模型 Key 只注入 Service 进程。容器入口脚本生成的 `/run/aiks/service.toml` 只保存环境变量**名称**，不会把 Secret 值写进去。

## systemd 部署

1. 复制 `service.toml.example` 到 `/etc/aiks/service.toml`；
2. 复制 `.env.example` 或另建 `/etc/aiks/team.env`，只保留 systemd 所需变量；
3. 填 CorpId、Client ID、回调、部门范围和服务器 Secret；
4. 将 `team.enabled=true`、`team.dingtalk.enabled=true`；
5. 执行 `/opt/aiks/bin/aiks-service --config /etc/aiks/service.toml --check-config`；
6. 安装 `aiks-service.service.example` 和 `nginx.conf.example` 后再启动。

系统服务方式下建议使用独立 `aiks` 用户、`UMask=0077`，数据库和 SiYuan workspace 都放到仅该服务账户可写的目录。

## Security boundary

- Team Service 只允许固定 numeric loopback listener；不要改成 `0.0.0.0`。
- Nginx 只代理明确的 AIKS API allowlist；不要增加 `/proxy/`、SQL、任意文件、SiYuan 原生接口。
- 不信任 `Forwarded` / `X-Forwarded-*` 来推导身份，模板会主动清空这些头。
- Client Secret、SiYuan Token、AI/Embedding Key 不进入桌面端、URL、日志、Git。
- `.env` 不是 shell 脚本，`deploy.sh` 不会 `source` 或执行其中内容。

## Backup / rollback

备份前应停止新写入、等待 AIKS 任务 drain、停止内部 SiYuan，然后对 SQLite 与 SiYuan workspace 做同一批次备份并记录 hash。恢复时先在新隔离目录验证，再切换原服务。不要让旧二进制直接打开已经升级 schema 的数据库。

## Validation status

CI 使用合成身份、临时数据库与合成 Secret。真实 DingTalk 企业授权、真实 SiYuan 恢复、桌面浏览器 callback、真实模型参数仍属于服务器联调证据，不能由 CI 绿色替代。
