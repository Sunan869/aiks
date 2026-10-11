# S2.1 真实模型提炼质量与性能基准验收入口

此工具与固定 JSON fixture 测试分离。**CI 默认只检查代码、评分逻辑与脱敏约束，不会连接模型端点**。没有执行此项命令前，不得将固定测试结果解释为真实模型质量或性能数据。

## 运行方式

在本地启用真实模型服务后，在项目根目录执行（PowerShell 示例）：

```powershell
$env:AIKS_BENCH_BASE_URL = "http://127.0.0.1:11434/v1"
$env:AIKS_BENCH_MODEL = "qwen3"
$env:AIKS_BENCH_REPEATS = "3"
$env:AIKS_BENCH_REPORT_PATH = "E:\AIKS-Test\live-benchmark-20261011.json"
cargo test -p aiks-core --test extraction_live_benchmark -- --ignored --nocapture
```

可选变量：`AIKS_BENCH_API_KEY` 与 `AIKS_BENCH_DISABLE_THINKING=1`。`AIKS_BENCH_REPEATS` 默认 1，最多 5。仅使用仓库中的合成匿名会话，不读实际 Provider 或 SiYuan 数据。端点由用户选择；如选择远程端点，合成样例仍会发送给该模型服务。

如果设置了 `AIKS_BENCH_REPORT_PATH`，程序只在**目标不存在**时新建 JSON 报告，拒绝覆盖旧报告；未设置则只在测试输出中打印汇总与匿名逐项统计。请将不同配置、模型、日期的报告分别保存在独立文件，不写入用户实际 Provider 目录。

## 统计含义和局限

- `requests` / `request_errors`：真实请求次数及请求失败数；网络失败不计作有效 JSON。
- `strict_json_valid`：直接反序列化成功数；`production_schema_valid`：生产解析与数值门禁通过数。两者不能当作事实正确率。
- `skip_matches`：是否符合样例应跳过/不应跳过标签，负例必须没有输出知识项。
- `term_hits` / `term_total`：合成样例指定技术关键词的启发式命中，**不是人工知识遗漏率，也不是事实准确率**。
- `exact_duplicate_titles`：同一响应内标题和类别都相同的重复数；不包括语义重复。
- `latency_p50_ms` / `latency_p95_ms`：每个真实 HTTP 模型请求测得的耗时，样例少时尾部分位不稳定；**不等于整个 Session 的提炼耗时**。
- `reported_token_calls` / `total_reported_tokens`：仅统计服务器实际提供的 usage total_tokens；未报告时为 null，不能用字符或估算 Token 冒充。
- `observations`：匿名样例 ID、重复次数、布尔门禁、计数和耗时，不包括原文、模型响应、Endpoint、Token 或 Key。

正常真实调用需要全部请求成功且模型输出通过生产 schema 门禁；**除此之外的关键词命中、跳过正确率与真实知识质量必须人工审阅和记录**。不要将这些固定合成样例当作真实项目知识质量结论。

## 模块验收补充

真实 Windows 机器上需配合 S1.1/S1.2 的内嵌 SiYuan 与真实 Session 操作验收；另记录模型版本、设备、参数、并发、长 Session 负载下的 P50/P95、Token、错误率、缓存命中率与人工遗漏评审。至少比较优化前后的同配置重复运行结果。任何一项缺失，都不得把 S2.1 标记为已完成实机验收。

对正例样例，`worth_extracting=true` 但 `items=[]` 的响应不能计入 `skip_matches`；对噪声负例，`worth_extracting=false` 时仍必须没有知识项。生产 Schema 可解析并不自动证明提炼结果符合业务预期。

## 超长 Prompt 与嵌套并发预算的专项故障注入（Windows 真机待执行）

使用独立 `AIKS_DATA_DIR` 与用户指定的测试模型端点，构造包含中文、Emoji、代码块、工具输出及 10 万字以上会话的合法源数据，先用 max_concurrent=1，然后依次 2/3/4 执行完整提炼。观测模型服务真实在途请求数、429、GPU/CPU 内存峰值、每阶段耗时及错误重试次数：**整体请求在途数不能因为 Session 并行与 Map-Reduce 片段并行相乘为 N²**。单 Session 多块处理在 N=2/3/4 时可以按显式允许的并行数工作；默认 1 不能意外并发。改变超时、max_context_tokens 或 max_tokens 时缓存指纹应正确失效。

对一个刻意过小的 `max_context_tokens`，预期 Pipeline 显式错误并展示可操作恢复说明，**不得伪造成功、静默跳过正文或修改旧 Knowledge**。对正常上下文但很长的 Prompt，检查实际发送文本遵循保护头部和最后一段的裁剪策略，省略标记本身在字符预算以内；显示中段被省略而非向模型声称已读取完整会话。实际模型 Token 数需要以模型服务 usage/服务端观测为准，字符预算只是保守估算，不保证所有 tokenizer 都严格 2 chars/token。

记录同一真实模型/参数/Session 下的首轮与重复提炼、缓存命中、批次 P50/P95、LLM calls/tokens、是否遗漏中段重要技术结论，并与 `extraction_live_benchmark.rs` 的单独真实模型结果交叉检查。该专项由用户在 Windows 真机执行，Rust 单测仅覆盖代码数学与拒绝路径，不构成真实效果验收；S1.3 完整备份最后执行。
