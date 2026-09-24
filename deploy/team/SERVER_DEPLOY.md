# AIKS 团队服务端镜像部署

本文描述 S2 single-company team 服务的生产部署方式：**本地构建镜像，上传镜像仓库，服务器只拉取镜像运行**。

服务器不需要 Rust、Cargo 或源码。

## 1. 部署结构

```text
Internet
   |
 HTTPS 443
   |
 Nginx
   |
   +---- 127.0.0.1:28081
          AIKS Service
             |
             +---- 127.0.0.1:6806
                    SiYuan

AIKS Service
   |
   +---- DingTalk HTTPS API
   +---- AI endpoint
   +---- Embedding endpoint
```

AIKS Service 与 SiYuan 均由 Docker Compose 管理，使用 host network 保持当前 loopback 安全约束。

## 2. 本地构建镜像

开发机执行：

```bash
docker build \
  -t registry.example.com/aiks/aiks-service:v0.1.0 \
  -f deploy/team/Dockerfile .

docker push registry.example.com/aiks/aiks-service:v0.1.0
```

SiYuan 使用官方镜像：

```text
b3log/siyuan:<version>
```

## 3. 服务器部署

服务器只需要：

- Docker Engine
- Docker Compose v2
- Nginx

进入目录：

```bash
cd deploy/team
./deploy.sh init
vim .env
```

配置镜像：

```dotenv
AIKS_IMAGE=registry.example.com/aiks/aiks-service:v0.1.0
SIYUAN_IMAGE=b3log/siyuan:latest
```

拉取并启动：

```bash
./deploy.sh pull
./deploy.sh check
./deploy.sh up
```

## 4. 关键配置

钉钉：

```dotenv
AIKS_DINGTALK_CORP_ID=
AIKS_DINGTALK_CLIENT_ID=
AIKS_DINGTALK_CLIENT_SECRET=
AIKS_DINGTALK_ROOT_DEPARTMENT_IDS=1
```

SiYuan：

```dotenv
AIKS_SIYUAN_BASE_URL=http://127.0.0.1:6806
AIKS_SIYUAN_TOKEN=
AIKS_SIYUAN_DATA_DIR=/data/aiks/siyuan
```

AIKS 数据：

```dotenv
AIKS_DATA_DIR=/data/aiks/service
```

模型首次联调建议关闭：

```dotenv
AIKS_AI_ENABLED=false
AIKS_EMBEDDING_ENABLED=false
```

## 5. 常用命令

```bash
./deploy.sh check
./deploy.sh pull
./deploy.sh up
./deploy.sh restart
./deploy.sh logs
./deploy.sh status
./deploy.sh down
```

升级流程：

```bash
docker push 新版本镜像

服务器:
./deploy.sh pull
./deploy.sh restart
```

## 6. 网络约束

禁止：

- 暴露 SiYuan 6806 到公网
- 暴露 AIKS Service 28081 到公网
- 增加 `/proxy/` 全代理
- 转发 SQL、文件接口

公网入口只允许：

```text
Nginx HTTPS
    |
    v
AIKS API allowlist
```

## 7. 验证顺序

1. `./deploy.sh check`
2. `/healthz` 本机验证
3. Nginx HTTPS 验证
4. 钉钉登录
5. 组织同步
6. ACL 分享
7. SiYuan owner-only 写入
8. AI / Embedding 联调

CI 使用合成身份和测试环境；真实企业钉钉、SiYuan 数据恢复、模型参数仍需服务器现场验证。
