# AIKS S2 钉钉配置填写说明

日期：2026-09-23。状态：**配置契约与样例，加载器/登录功能尚未实现。**
本说明属于已确认设计的实施准备，不代表现在填好就能登录。

## 1. 文件位置

本次已经提供的可填写空白样例：

- `docs/implementation/examples/team-service.toml.example`
- `docs/implementation/examples/team-secrets.env.example`

实施计划约定，配置加载器交付时在 `deploy/team/` 提供相同键名的正式模板。
部署者将它们复制为 `deploy/team/service.toml` 和 `deploy/team/.env`，也可通过显式
`--config` 使用仓库以外的配置路径。**不要修改个人空间的
`service-local/config/models.toml` 来配置钉钉。** 不自动发现/执行当前目录中的 .env。

## 2. 后续要手动填写的内容

| 位置 | 键 | 填写内容 |
| --- | --- | --- |
| service.toml | `team.public_base_url` | AIKS 团队服务固定 HTTPS origin，例如 `https://aiks.example.com`；不是思源或 Vite 地址 |
| service.toml | `team.dingtalk.corp_id` | 公司 CorpId，绑定单一组织 |
| service.toml | `team.dingtalk.client_id` | 企业内部应用 Client ID／AppKey |
| service.toml | `team.dingtalk.redirect_uri` | 上述 origin 加 `/api/v1/auth/dingtalk/callback`；与钉钉后台注册值一致 |
| 服务端环境 | `AIKS_DINGTALK_CLIENT_SECRET` | Client Secret／AppSecret，只在服务端填真实值 |
| service.toml | `team.directory.root_department_ids` | 初期获授权的测试部门节点 ID 列表；空列表不代表全公司 |
| 服务端环境 | `AIKS_SIYUAN_TOKEN` | 内部思源实例的访问凭据，不是钉钉密钥 |

Client Secret 也可以不使用环境变量：把 `client_secret_env` 设为 `""`，
把 `client_secret_file` 设为仅服务账户可读的绝对文件路径，将密钥写入该文件。
**环境引用与文件引用只选一个；同时填写报错，不静默选择。**
秘密文件接受单行 UTF-8，最多 8 KiB；去掉唯一的末尾 CRLF/LF，其余前后空格或控制字符报错。
拒绝链接、目录、不安全权限及空内容。Windows 权限校验使用 ACL，不把 Unix mode 当作 Windows 安全保证。
这些是本项目拟实现的读取约束，不是钉钉提供的配置功能。

## 3. 填写示例（占位值，不是真实部署）

```toml
[team]
enabled = false
public_base_url = "https://aiks.example.com"

[team.dingtalk]
enabled = false
corp_id = ""
client_id = ""
redirect_uri = "https://aiks.example.com/api/v1/auth/dingtalk/callback"
client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET"
client_secret_file = ""
```

保留 `enabled = false`，直到代码、配置检查、通讯录授权和联调都通过。
开发阶段不需要向聊天或 GitHub 提供任何密钥：自动化测试使用临时数据库及合成钉钉响应，
不能以“没有真实凭据”为由加入假登录、默认管理员或匿名团队访问。

## 4. 加载与缺项时的行为（待实现契约）

- 个人模式不加载钉钉秘密、不请求钉钉、不依赖组织服务；缺少全部钉钉配置仍然可用。
- 显式选择 team 但未启用或缺项：启动前给出字段名/安全错误码，不打开业务监听，
  不创建或接管个人数据库，不回退为 personal。
- `--check-config` 只校验文件、秘密引用和固定 URL，**不打开数据库、不监听端口、不访问钉钉**。
  它的成功不代表应用权限或 OAuth 已联调成功。
- 即使静态配置齐全，组织首次同步未成功时也不签发可访问业务的团队会话。
- 不允许配置任意钉钉 API URL；生产端点固定在适配器内。测试替身通过代码注入，不能由生产配置放开任意 origin。
- 一个非空公司库持久绑定 CorpId/身份提供方；以后改 CorpId/Client ID 要明确报错并提示受控重新绑定，不能把旧知识变成新公司的数据。
- 配置修改后重启团队服务。升级保留真实配置，不覆盖密钥；个人模式启动脚本保持原行为。
- `.env` 不是 shell 脚本，不使用 `eval`、PowerShell `Invoke-Expression` 或 `source` 执行不可信文本。
  将来 Compose 显式使用 `env_file`；裸二进制或系统服务由部署者注入进程环境。

## 5. 安全与独立验收

回调属于 AIKS Service，不属于思源。浏览器必须能访问这个回调；服务必须能出站访问钉钉。
拟定生产入口使用 HTTPS，反向代理保留静态配置的 Host，不信任客户端任意 forwarded headers。
思源只在内部网络可达。钉钉登录不能作为完全断网登录使用；个人离线版保持独立。

真实 Secret 不进入前端/Webview、桌面安装包、上传队列、回执、日志、文档或 Git。
`--check-config` 只输出“未配置/有效/无效”及字段名，不输出秘密内容或其尾号。
后续真实企业测试由部署者手动注入凭据完成，测试结果独立于合成响应的 CI 结果记录。

## 6. 官方依据与项目约定的区别

钉钉的 Client ID/Secret、授权码流程、`authCode` 回调字段与 redirect URI 注册匹配要求参考：
https://opensource.dingtalk.com/developerpedia/docs/develop/permission/token/browser/get_user_app_token_browser/

Client Secret 的保密属性参考：
https://opensource.dingtalk.com/developerpedia/docs/learn/permission/intro/permission-glossary/

**本文件中的路径、TOML 键、环境变量名、启用开关、同步间隔和错误行为均为 AIKS 的配置契约，
不是钉钉后台原生选项。** 本次只提供说明与可解析空样例，没有修改 S1 运行代码。
