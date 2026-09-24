# AIKS 团队服务端 Docker Compose 部署

本文对应 `feature/aiks-service-extraction` 的 S2 single-company team 服务，适合先部署到一台 Linux 服务器做真实钉钉、SiYuan、AI/Embedding 联调。个人离线模式不使用这套 Compose。

## 1. 部署结构

```text
Internet
   |
 HTTPS 443
   v
宿主机 Nginx
   |
   +--> 127.0.0.1:28081  AIKS Service（Docker host network）
   |
   X    不代理 SiYuan

宿主机 127.0.0.1:6806  SiYuan
AIKS Service ----------> SiYuan
AIKS Service ----------> DingTalk HTTPS API
AIKS Service ----------> AI / Embedding endpoint（启用时）
```

当前服务端安全契约要求 AIKS Service 和 SiYuan 都使用 numeric loopback。Docker 因此使用 `network_mode: host`，但 AIKS 仍只监听 `127.0.0.1`，Compose **没有 `ports:` 公网映射**。公网入口必须由宿主机 Nginx/TLS 提供。

## 2. 服务器准备

建议环境：Linux x86_64、Docker Engine 24+、Docker Compose v2、可用的 HTTPS 域名和证书。SiYuan 需要已经在宿主机 `127.0.0.1:6806` 可访问；不要将 6806 暴露到公网。

进入仓库：

```bash
git checkout feature/aiks-service-extraction
cd deploy/team
chmod +x deploy.sh
./deploy.sh init
```

`init` 只在 `.env` 不存在时从 `.env.example` 创建，不会覆盖已有配置。随后：

```bash
chmod 600 .env
vim .env
```

## 3. 必填参数

真实值只填在服务器 `.env`，不要提交 Git。

| 变量 | 说明 |
| --- | --- |
| `AIKS_PUBLIC_HOST` | 对外 HTTPS authority，例如 `aiks.example.com` |
| `AIKS_PUBLIC_BASE_URL` | 必须与上面一致，例如 `https://aiks.example.com` |
| `AIKS_DINGTALK_CORP_ID` | 当前企业 CorpId |
| `AIKS_DINGTALK_CLIENT_ID` | 企业内部应用 Client ID |
| `AIKS_DINGTALK_CLIENT_SECRET` | 企业内部应用 Client Secret |
| `AIKS_DINGTALK_ROOT_DEPARTMENT_IDS` | 首批允许同步的部门 ID，多个用逗号分隔 |
| `AIKS_SIYUAN_TOKEN` | 服务器内部 SiYuan API Token |

钉钉回调地址由容器自动生成：

```text
${AIKS_PUBLIC_BASE_URL}/api/v1/auth/dingtalk/callback
```

必须与钉钉后台配置完全一致。

## 4. AI / Embedding 参数

第一次服务器联调可以都保持：

```dotenv
AIKS_AI_ENABLED=false
AIKS_EMBEDDING_ENABLED=false
```

这样可以先验收登录、组织目录、ACL、分享、内容与导入链路。模型准备好后再填写：

```dotenv
AIKS_AI_ENABLED=true
AIKS_AI_BASE_URL=http://10.10.23.16:18000/v1
AIKS_AI_MODEL=实际模型名
AIKS_AI_API_KEY=实际Key或留空

AIKS_EMBEDDING_ENABLED=true
AIKS_EMBEDDING_BASE_URL=http://10.10.23.16:28090/v1
AIKS_EMBEDDING_MODEL=实际模型名
AIKS_EMBEDDING_API_KEY=实际Key或留空
```

如果 Key 留空，容器生成的 TOML 不会声明对应 Key 环境引用；如果填写，则只通过环境变量注入，不写入生成的 TOML。

## 5. 配置检查

正式启动前执行：

```bash
./deploy.sh check
```

它会依次执行：Compose 解析、镜像构建、容器内运行 `aiks-service --check-config`。这个检查不会访问钉钉或模型，也不会打开业务数据库/监听业务端口。

成功应看到：

```text
configuration_valid
```

常见错误：

- `invalid_identity`：CorpId / Client ID 仍为空或格式错误；
- `secret_missing`：Client Secret、SiYuan Token 或启用模型所引用的 Key 缺失；
- `callback_mismatch`：公网域名与回调不一致；
- `directory_scope_required`：未配置部门范围；
- `internal_loopback_required`：SiYuan 被改成了非 `127.0.0.1` 地址。

## 6. 启动与验证

```bash
./deploy.sh up
./deploy.sh status
./deploy.sh logs
```

宿主机本地验证时必须带正确 Host：

```bash
curl -sS -H 'Host: aiks.example.com' http://127.0.0.1:28081/healthz
```

预期：

```json
{"mode":"team","status":"ok"}
```

随后将 `nginx.conf.example` 中的域名和证书路径替换成真实值，安装到宿主机 Nginx。不要增加 `/proxy/`、SiYuan、SQL、任意文件等转发规则，也不要向 Service 传递客户端提供的 `Forwarded` / `X-Forwarded-*` 身份头。

公网验证：

```bash
curl -sS https://你的域名/healthz
```

## 7. 常用命令

```bash
./deploy.sh check
./deploy.sh build
./deploy.sh up
./deploy.sh restart
./deploy.sh down
./deploy.sh logs 300
./deploy.sh status
```

更新功能分支代码后使用 `./deploy.sh restart`，脚本会先重新执行配置检查，再 build + force recreate。

## 8. 数据目录

默认数据目录：

```text
deploy/team/data/
  business/
    state.db
    state.db-wal
    state.db-shm
```

可以通过 `.env` 的 `AIKS_DATA_DIR` 改为宿主机绝对目录，例如：

```dotenv
AIKS_DATA_DIR=/data/aiks-team
```

不要把正在运行的 SQLite 主文件单独复制当备份。备份前停止新写入并 drain 服务，再与 SiYuan workspace 做同一批次备份；恢复也必须成对恢复。

## 9. Nginx / TLS

`nginx.conf.example` 是 allowlist 反代模板。部署时至少替换：

- `server_name aiks.example.com`；
- `ssl_certificate`；
- `ssl_certificate_key`；
- `proxy_set_header Host aiks.example.com`。

`proxy_set_header Host` 必须与 `.env` 的 `AIKS_PUBLIC_HOST` 相同。AIKS Service 会拒绝 Host 不一致、Origin/Forwarded 伪造等请求。

## 10. 真实联调顺序

建议按以下顺序，便于定位问题：

1. `./deploy.sh check` 通过；
2. `/healthz` 本机通过；
3. Nginx HTTPS `/healthz` 通过；
4. 钉钉登录 start → 浏览器 → callback → desktop exchange；
5. 首次组织目录同步完成；
6. A 用户创建/导入知识，B 用户默认 404；
7. A 分享给 B 或部门，B 可读但编辑/分享返回 403；
8. 撤销分享后 B 再次不可读；
9. 验证 SiYuan owner-only 写入与冲突恢复；
10. 最后再启用 AI / Embedding，验证提炼与搜索。

CI 当前使用合成身份与合成上游。上述第 4、5、9、10 项是真实环境联调证据，不能由 CI 绿色替代。
