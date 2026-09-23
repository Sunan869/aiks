# AI 和向量配置归 AIKS Service 管理

日期：2026-09-23。本次实现服务器模型环境凭据读取；钉钉登录和团队权限仍在 S2 计划中，未开放远程模式。

## 配置职责

团队模式：AI 提炼模型、Embedding 模型、地址、模型名称、维度、分块、并发和超时由服务器管理。普通客户端显示能力状态，不保存公司模型密钥，也不按客户端单独生成不同维度的共享向量。公司服务可以调用公司内网模型或经允许的外部模型；配置在服务器不等于模型本身必须在同一台机器。

个人离线模式：仍由本机 Service 使用独立的本机模型配置。现有 `service-local/config/models.toml` 不被覆盖，不因为连接公司而继承公司密钥或自动上传本地资料。完全离线需要提前准备本机模型和运行资源。

## 当前可用的独立 Service 配置增量

独立 `aiks-service --config <文件>` 在原有 `[ai]`、`[embedding]` 之外支持 `[model_credentials]`。本段属于 Service 启动配置，不是要求在桌面 `models.toml` 增加该段（桌面配置读写的扩展另行接线）。本期已有个人配置继续有效。

```toml
[model_credentials]
ai_api_key_env = "AIKS_AI_API_KEY"
embedding_api_key_env = "AIKS_EMBEDDING_API_KEY"
```

这里填写变量名称，真实值由部署者注入 Service 进程环境，两个模型可以使用不同密钥。
AI/Embedding 的 URL 和名称仍分别填写在 `[ai]`/`[embedding]`。没有鉴权的本机/内网模型可将对应环境引用设为 `""`；不要求虚构 API Key。

未启用模型时不读取其秘密变量。模型启用且引用不为空时，变量缺失或无效就启动失败，失败发生在打开业务库和监听端口之前。错误只含固定字段名与错误码，不输出变量值。变量值为 1–8192 字节可打印 ASCII，不允许空格、换行或控制字符。变量名最多 128 字节，采用字母/下划线开头及字母、数字、下划线。

旧个人配置的 `api_key` 保持兼容；同一个已启用模型不能同时配置非空环境引用和 inline `api_key`（包括空字符串 inline），会报告 `ambiguous_secret_source`，不静默选择某一份。解析过程对两项模型凭据原子应用，后者失败不会留下前者的半配置状态。

修改后重启 Service。不自动发现或执行 `.env`，不调用 shell 执行配置。未来 Compose 的 `env_file` 或服务管理器负责环境注入。真实 `.env` 不提交 Git；本次仓库中只有空白样例。

## 团队模板与后续边界

`docs/implementation/examples/team-service.toml.example` 同时预留钉钉、AI、Embedding 配置。
`docs/implementation/examples/team-secrets.env.example` 预留四种服务端秘密：钉钉、思源、AI、Embedding。
整份 team 模板依旧是后续团队加载器的契约，当前 Service 不接受 `mode=team`，不能因模型凭据功能已加入而直接开放团队服务。

切换向量模型、维度或分块策略需要受控重建索引，原始知识不删除。不能把不兼容的新旧向量混在一起查询。本次没有实现向量模型热切换或自动索引迁移，不把改配置说成迁移已完成。

首版先使用服务器文件配置，不增加普通用户修改公司模型的入口。后续需要可视化管理时，也只允许授权的配置管理员修改，并不因此赋予管理员读取所有私有知识的权限。
