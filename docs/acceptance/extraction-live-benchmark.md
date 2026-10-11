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

## 单块 Session 持久提炼缓存专项（Windows / 真实模型待验收）

以前只在长会话 Map-Reduce 分块重用 SQLite 提炼缓存，短会话单块在再次处理时总是重复调用 LLM。现在单块与多块共用原有 `knowledge-chunk-v5` 内容指纹与本地 SQLite 缓存；模型端点、模型名、temperature、max_tokens、max_context_tokens、disable_thinking、Session 标题、项目和原始块内容共同决定命中条件。旧 v5 合法缓存继续兼容。

在独立 AIKS 测试数据目录下，选取同一真实短 Session、相同模型和参数连续提炼两次，第二次应有 `cache_hits=1`、`llm_calls=0`，且不得因为纯缓存命中伪造新增的 `ai_request_log` 或 Token 使用量；重启后重复验证。再追加消息、更换项目/标题、改模型或上下文窗口，确认缓存失效并重新向服务发送请求。对自相矛盾的响应（宣称需要提炼却给空 items，或宣称跳过却给非空 items）不能持久化为成功缓存；手工修改 SiYuan 原文必须继续被保护。缓存命中仅代表相同提炼结果重用，不证明事实准确、知识没有遗漏，仍需人工复核。

Rust 自动化只验证缓存身份、SQLite 持久性和失效规则，不连接实际模型，不能替代 Windows 真机、真实模型或异常中断测试。S1.3 完整备份/恢复继续在所有其他功能代码完成后最后实施。

## 2026-10-11 提炼结果一致性、旧缓存指纹与真实重试专项（待 Windows/模型执行）

先在独立的 `AIKS_DATA_DIR` 中准备一个约 1–2 块的合成 Session。使用用户自行配置的真实模型，先完成一次正常提炼并保留本地 `state.db` 及阶段日志；重启 Desktop、保持模型与参数不变重复提炼。单块缓存的 key 必须与原 Map-Reduce `knowledge-chunk-v5` 一字节不变，第二次同配置应命中磁盘缓存。调整模型、项目、Session 标题或消息内容后要重新请求模型，不能误用已有结果，也不能自动覆盖 SiYuan 中用户编辑的原文。

用本地可控模型测试端点**故障注入**两种完全可解析但语义冲突的 JSON：`worth_extracting=true, items=[]` 和 `worth_extracting=false, items=[合法知识条目]`。生产提炼必须明确失败并允许后续重新处理，而不能把前者当作「成功提炼 0 条」、把后者当作正常无知识，亦不能写入缓存。有效 `worth_extracting=false, items=[]` 仍应正常记为跳过；有效非空结果仍可缓存。中途截断但被容错 JSON 解析器恢复为「worth=true，只有空 items」的输出，同样需要报出可识别的恢复建议（提高 max_tokens/修正模型响应），而不是悄悄丢失知识。生产解析对这类不一致输出的拒绝与纯粹能否反序列化 JSON 是不同指标。

记录请求次数、`llm_calls`、`cache_hits`、重复运行内容一致性、失败日志和结果是否变更；不能单凭 CI Rust 固定值或本地伪响应声称真实模型的质量和失败恢复都已验收。完整备份与恢复 S1.3 仍在全部其他模块完成之后实施。
