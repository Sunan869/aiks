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
