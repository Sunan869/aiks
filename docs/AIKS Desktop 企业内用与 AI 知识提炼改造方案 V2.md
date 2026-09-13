# AIKS Desktop 企业内用与 AI 知识提炼改造方案 V2

版本：V2.0  
适用项目：AI Knowledge Sync / AIKS Desktop  
目标用户：公司内部研发人员、算法工程师、测试人员、技术管理人员  
目标：将当前 AIKS 从“AI 会话同步工具”升级为“企业内部个人 AI 工作知识库”

---

# 1. 本轮改造目标

当前 AIKS 已具备：

- OpenCode / Codex / Gemini / Claude Code 数据采集能力
- Canonical Session Model
- Session Scanner
- Incremental Sync
- File Watcher
- Embedded SiYuan Runtime
- Tauri Desktop
- System Tray
- Session → Markdown → SiYuan
- 本地 SQLite State
- 660 条真实 Session 扫描验证
- Windows Installer

当前主要问题已经从“技术链路是否能跑”转变为：

```text
员工是否愿意长期使用？
```

当前存在两类核心问题：

## 1.1 产品体验问题

目前：

```text
AIKS
+
AIKS Knowledge
```

启动后出现两个窗口。

普通用户很难理解：

```text
哪个是主程序？
哪个是知识库？
关闭哪个会退出？
```

同时当前页面仍有较多：

```text
Runtime
Kernel
Port
SiYuan
Provider
```

等技术概念，不适合作为公司内部正式工具。

---

## 1.2 知识质量问题

目前流程主要是：

```text
OpenCode Session
        ↓
NormalizedSession
        ↓
Markdown
        ↓
SiYuan
```

这种方式解决了：

```text
AI 对话不会丢
```

但没有真正解决：

```text
AI 对话如何变成可复用知识
```

OpenCode / Codex Session 中往往包含大量：

- 重复问答
- 中间尝试
- Tool Call
- Shell 输出
- 编译日志
- 错误信息
- 多轮修改
- 无效方案
- 长代码
- 上下文补充

原样同步后：

```text
信息完整
```

但往往：

```text
太长
太碎
不够简洁
难快速复用
```

因此本轮增加：

```text
AI Knowledge Extractor
```

利用公司内部 vLLM 模型将 Session 转换为更高质量的知识文档。

---

# 2. 本轮整体产品定位

AIKS 不再定位为：

```text
AI 聊天记录同步工具
```

推荐正式定位：

> AIKS 是员工个人的 AI 工作知识库，自动沉淀 OpenCode、Codex、Gemini、Claude Code 等 AI 工具中的工作过程，并利用公司内部 AI 模型将原始对话整理为可搜索、可复用、可持续积累的工作知识。

核心价值不是：

```text
保存聊天记录
```

而是：

```text
让 AI 工作不再用完即丢。
```

---

# 3. 改造后的完整架构

最终：

```text
OpenCode
Codex
Gemini
Claude Code
     │
     ▼
Session Providers
     │
     ▼
Canonical Session Model
     │
     ├─────────────────────────┐
     │                         │
     ▼                         ▼
Raw Session Sink        Knowledge Extractor
     │                         │
     ▼                         ▼
10 AI Sessions          Qwen3.8-27B
                               │
                               ▼
                        KnowledgeDocument
                               │
                               ▼
                         20 Knowledge
```

最终知识体系：

```text
AI Knowledge
│
├── 10 AI Sessions
│   ├── OpenCode
│   ├── Codex
│   ├── Gemini
│   └── Claude
│
├── 20 Knowledge
│   ├── Projects
│   ├── Troubleshooting
│   ├── Implementation
│   ├── Design
│   ├── Research
│   └── Decisions
│
└── 90 System
```

---

# 4. 核心原则：原始 Session 永远保留

AI 模型不得替代原始 Session。

禁止：

```text
Session
  ↓
AI 总结
  ↓
删除原始 Session
```

必须：

```text
                 ┌─ 原始 Session 文档
NormalizedSession
                 └─ AI 精炼知识文档
```

原因：

- AI 总结一定存在信息损失
- 模型可能错误理解
- 后续模型升级后需要重新提炼
- 原始上下文是最终依据
- 某些代码、错误、命令可能未来重新有价值

因此：

```text
Raw Session = Source of Truth
Knowledge Document = Derived Artifact
```

---

# 5. 本轮本地 AI 模型

公司现有 vLLM：

```text
Base URL:
http://10.10.23.16:18000/v1

Model:
Qwen3.8-27B
```

AIKS 第一版 Knowledge Extractor 默认使用该模型。

模型主要承担：

```text
分类
去噪
摘要
标签生成
项目识别
问题提取
根因提取
解决方案提取
技术决策提取
关键命令提取
待办提取
知识价值判断
```

该场景不需要特别强的开放式推理。

27B 模型足够作为第一版内部知识整理模型。

---

# 6. AI Provider 不得写死 Qwen

虽然当前使用：

```text
Qwen3.8-27B
```

但代码层必须抽象为：

```text
OpenAI Compatible AI Provider
```

新增配置：

```rust
pub struct AiModelConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
}
```

默认：

```text
enabled = true

base_url =
http://10.10.23.16:18000/v1

model =
Qwen3.8-27B

temperature =
0.1

api_key =
None
```

---

# 7. AI API

第一版优先使用：

```http
POST /v1/chat/completions
```

完整：

```text
http://10.10.23.16:18000/v1/chat/completions
```

请求：

```json
{
  "model": "Qwen3.8-27B",
  "temperature": 0.1,
  "messages": [
    {
      "role": "system",
      "content": "..."
    },
    {
      "role": "user",
      "content": "..."
    }
  ]
}
```

未来可以支持：

```text
/v1/responses
```

但第一版不需要同时实现两套协议。

---

# 8. 增加 AI 模块

建议：

```text
crates/aiks-core/src/
│
├── ai/
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   ├── extractor.rs
│   ├── schema.rs
│   ├── chunker.rs
│   ├── prompts.rs
│   └── error.rs
```

职责：

```text
client.rs
OpenAI-compatible HTTP Client

extractor.rs
Session → Knowledge Extraction

chunker.rs
长 Session 分块

schema.rs
结构化输出 Schema

prompts.rs
Prompt Version

config.rs
模型配置
```

---

# 9. Knowledge Extractor 输出必须结构化

不得让模型自由生成 Markdown 后直接保存。

要求模型输出 JSON。

建议：

```json
{
  "worth_extracting": true,
  "knowledge_score": 0.87,

  "title": "AIKS Embedded SiYuan Runtime 集成修复",

  "summary": "完成 AIKS 内嵌 SiYuan Runtime 从版本锁定、运行资源准备到 Kernel 启动和 Tauri 打包的完整修复。",

  "project": "AIKS",

  "category": "troubleshooting",

  "tags": [
    "Tauri",
    "SiYuan",
    "Rust",
    "PowerShell"
  ],

  "problem": "AIKS Embedded SiYuan 的构建与启动链路连续出现版本、脚本和 Runtime 资源问题。",

  "symptoms": [
    "siyuan.version 无法解析",
    "GitHub Release Asset 下载失败",
    "PowerShell warning 被误判为构建失败"
  ],

  "root_causes": [
    "PowerShell 使用 -contains 判断字符串是否包含等号",
    "Windows Release 实际为 NSIS EXE 而非 ZIP",
    "PowerShell 5.1 会将 Native stderr 包装成 NativeCommandError"
  ],

  "solutions": [
    "固定 SiYuan 3.8.3",
    "修复 siyuan.version parser",
    "使用官方 NSIS Windows Asset",
    "校验 SHA256",
    "使用统一 NativeCommand wrapper"
  ],

  "decisions": [
    "Embedded Runtime 不要求 SIYUAN_TOKEN",
    "Kernel 仅监听 127.0.0.1",
    "Runtime 在构建阶段打入安装包"
  ],

  "key_commands": [
    ".\\scripts\\setup-siyuan.ps1",
    ".\\scripts\\build-release.ps1"
  ],

  "key_files": [
    "scripts/setup-siyuan.ps1",
    "scripts/build-release.ps1",
    "crates/aiks-core/src/runtime/mod.rs"
  ],

  "todos": [],

  "confidence": 0.94
}
```

---

# 10. Knowledge Category

第一版限定：

```text
troubleshooting
implementation
design
research
decision
general
```

不要无限开放分类。

---

# 11. 不同类型生成不同文档

## Troubleshooting

```text
问题

现象

根因

解决方案

验证方式

关键命令

注意事项
```

---

## Implementation

```text
目标

实现方案

关键模块

关键代码

关键文件

验证结果

后续事项
```

---

## Design

```text
背景

目标

总体架构

关键设计决策

方案权衡

最终方案

待解决问题
```

---

## Research

```text
研究问题

候选方案

比较

结论

参考信息

后续方向
```

---

## Decision

```text
决策背景

最终决策

为什么这样做

未采用方案

影响范围
```

---

# 12. Knowledge Score

不是每一个 Session 都值得生成精炼知识。

例如：

```text
帮我改个变量名
```

这种 Session：

```text
knowledge_score = 0.2
```

只保存 Raw Session。

例如：

```text
AIKS Embedded SiYuan Runtime 修复
```

这种：

```text
knowledge_score = 0.9
```

进入 Knowledge。

建议阈值：

```text
0.6
```

流程：

```text
Session
  ↓
Extractor
  ↓
worth_extracting
  ↓
false → Only Raw Session
true  → Generate Knowledge
```

---

# 13. Session 触发时机

不要每增加一个 Message 就调用模型。

错误：

```text
Message 1
→ AI Extract

Message 2
→ AI Extract

Message 3
→ AI Extract
```

推荐：

```text
Session Updated
       ↓
Watcher
       ↓
Debounce
       ↓
5～10 分钟无新消息
       ↓
Knowledge Extraction
```

默认：

```text
10 分钟未变化
```

即可开始整理。

---

# 14. 手动立即整理

Knowledge 页面和 Session 页面增加：

```text
[立即整理]
```

用于：

```text
Session 尚未自然结束
```

但用户已经希望提炼。

---

# 15. 长 Session 分块

不能直接把数百条 Message 全部一次提交给模型。

新增：

```text
Chunker
```

第一版建议按：

```text
20,000～30,000 tokens / chunk
```

或者：

```text
30～50 messages
```

双重限制。

流程：

```text
Session
  ↓
Chunk 1
Chunk 2
Chunk 3
Chunk 4
  ↓
Chunk Summaries
  ↓
Final Extraction
```

即：

```text
Map
↓
Reduce
```

---

# 16. Chunk Summary

中间 Summary 不直接写入知识库。

内部结构：

```json
{
  "chunk_index": 1,
  "summary": "...",
  "important_errors": [],
  "decisions": [],
  "commands": [],
  "files": [],
  "todos": []
}
```

最后：

```text
所有 Chunk Summary
        ↓
Final Knowledge Extractor
```

---

# 17. 不能过度压缩的重要信息

Prompt 必须明确要求优先保留：

```text
错误信息

最终解决命令

接口路径

URL

配置项

类名

函数名

文件路径

版本号

模型名

数据库表

API

关键代码片段

性能数据

架构决策

失败原因
```

尤其代码类 Session。

---

# 18. 可以压缩的内容

允许模型压缩：

```text
重复解释

重复错误日志

多次相同尝试

闲聊

模型自我重复

无意义 stdout

巨大的 Tool Result

已经被后续方案完全替代的中间步骤
```

---

# 19. Thinking

公司正式使用建议：

```text
Thinking 默认不进入 Knowledge Extractor
```

除非用户显式打开：

```text
高级设置
→ 使用 Thinking 辅助知识整理
```

默认：

```text
OFF
```

---

# 20. Tool Call

保留：

```text
Tool Name
Input
关键 Output
```

巨大 Tool Result 必须截断。

例如：

```text
最大 100 KB
```

超过：

```text
前 30 KB
+
后 30 KB
+
[中间内容已截断]
```

---

# 21. Knowledge Renderer

AI 模型输出 JSON。

AIKS 负责：

```text
JSON
↓
Markdown Renderer
↓
SiYuan
```

不要让 LLM 自由控制最终 Markdown 格式。

这样可以保证：

```text
文档格式稳定

后续可重新渲染

更换模型不影响格式

UI 可做结构化展示
```

---

# 22. Knowledge Markdown 示例

```markdown
# AIKS Embedded SiYuan Runtime 集成修复

> 来源：OpenCode  
> 项目：AIKS  
> 类型：故障排查  
> 日期：2026-09-12

## 摘要

完成 AIKS 内嵌 SiYuan Runtime 的完整修复。

## 问题

...

## 根因

...

## 解决方案

...

## 关键命令

...

## 关键文件

...

## 设计决策

...

## 后续事项

...

---

原始会话：

[[OpenCode Session - xxxxx]]
```

---

# 23. Raw Session 与 Knowledge 双向关联

Raw Session 增加：

```text
custom-aiks-knowledge-id
```

Knowledge 增加：

```text
custom-aiks-source-session-id
```

做到：

```text
知识文档
→ 查看原始会话

原始会话
→ 查看精炼知识
```

---

# 24. Knowledge Extraction State

新增 SQLite 表：

```sql
CREATE TABLE knowledge_extraction (
    id INTEGER PRIMARY KEY,

    source TEXT NOT NULL,
    external_session_id TEXT NOT NULL,

    source_content_hash TEXT NOT NULL,

    extractor_version TEXT NOT NULL,
    prompt_version TEXT NOT NULL,

    model TEXT NOT NULL,
    model_endpoint TEXT,

    knowledge_score REAL,

    category TEXT,

    status TEXT NOT NULL,

    knowledge_document_id TEXT,

    knowledge_hash TEXT,

    error_message TEXT,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    UNIQUE(source, external_session_id)
);
```

---

# 25. Extraction Status

支持：

```text
PENDING

RUNNING

SUCCESS

SKIPPED

FAILED

STALE
```

---

# 26. 增量重提炼

只有：

```text
source_content_hash
```

变化时，才重新整理。

如果：

```text
source hash unchanged
```

不要再次调用模型。

---

# 27. Prompt Version

必须保存：

```text
prompt_version
```

例如：

```text
knowledge-extractor-v1
```

以后 Prompt 改成：

```text
knowledge-extractor-v2
```

允许：

```text
重新整理历史知识
```

---

# 28. Extractor Version

例如：

```text
extractor-v1
```

如果 chunk 算法、分类逻辑发生重大变化：

```text
extractor-v2
```

方便追踪。

---

# 29. Model 变化

数据库中记录：

```text
Qwen3.8-27B
```

未来改成其他模型以后：

允许：

```text
重新整理
```

而不修改 Raw Session。

---

# 30. AI 失败时不能影响同步

非常重要。

流程：

```text
Raw Session Sync
       ↓
成功
       ↓
Knowledge Extractor
```

如果模型：

```text
超时
不可访问
JSON 格式错误
模型异常
```

必须：

```text
Raw Session 仍然同步成功
```

Knowledge Extraction 单独：

```text
FAILED
```

不能导致整个 AIKS Sync Failed。

---

# 31. AI Queue

不要同步线程里直接等待模型。

新增：

```text
Knowledge Extraction Queue
```

例如：

```text
Session Sync
   ↓
enqueue extraction
   ↓
background worker
   ↓
AI Model
```

第一版：

```text
并发 1～2
```

足够。

---

# 32. 重试策略

模型失败：

```text
Retry 1
30 秒

Retry 2
2 分钟

Retry 3
10 分钟
```

之后：

```text
FAILED
```

用户可以：

```text
[重试整理]
```

---

# 33. 模型健康检查

增加：

```text
AI Model Health
```

启动后：

```text
GET /v1/models
```

验证：

```text
Qwen3.8-27B
```

是否存在。

如果：

```text
/v1/models
```

不可用，则允许 fallback：

调用最小：

```text
chat/completions
```

进行测试。

---

# 34. 设置页 AI 区域

新增：

```text
设置
└── AI 智能整理
```

公司普通员工看到：

```text
AI 智能整理

● 已启用

模型：
公司内部 AI

自动整理新会话        ON

自动生成摘要          ON

提取问题与解决方案    ON

提取设计决策          ON

自动生成标签          ON

[测试连接]
```

---

# 35. URL 默认隐藏

公司内部员工不需要看到：

```text
http://10.10.23.16:18000/v1
```

普通 UI 显示：

```text
公司内部 AI
```

高级设置：

```text
服务地址：
http://10.10.23.16:18000/v1

模型：
Qwen3.8-27B
```

---

# 36. Enterprise Policy

新增：

```text
enterprise-policy.json
```

例如：

```json
{
  "ai": {
    "enabled": true,
    "baseUrl": "http://10.10.23.16:18000/v1",
    "model": "Qwen3.8-27B",
    "allowUserOverride": false
  },

  "security": {
    "saveThinking": false,
    "secretSanitizer": true
  },

  "sync": {
    "autoSync": true
  }
}
```

---

# 37. 配置优先级

```text
Enterprise Policy
        ↓
User Settings
        ↓
Defaults
```

如果：

```text
allowUserOverride=false
```

普通员工不能修改模型 URL。

---

# 38. 数据安全

公司内用建议明确显示：

```text
AIKS 默认在本机保存数据。
```

Knowledge Extraction：

```text
本机
↓
公司内网
↓
Qwen3.8-27B
```

不访问外部模型。

这是重要产品能力。

---

# 39. 发送模型前仍需 Sanitizer

流程：

```text
NormalizedSession
       ↓
Secret Sanitizer
       ↓
Knowledge Extractor
```

即使模型在内网，也继续脱敏：

```text
Bearer Token

Authorization

API Key

SecretKey

Password

AccessToken

数据库密码
```

---

# 40. 单窗口产品改造

启动后只显示：

```text
AIKS
```

不再自动显示：

```text
AIKS Knowledge
```

---

# 41. 主导航

推荐：

```text
概览

知识库

数据源

同步记录

────────

设置

帮助与诊断
```

---

# 42. 主窗口布局

```text
┌────────────────────────────────────────────────────────────┐
│ AIKS                                搜索      状态          │
├──────────────┬─────────────────────────────────────────────┤
│              │                                             │
│ 概览         │                                             │
│              │                                             │
│ 知识库       │             主内容区域                      │
│              │                                             │
│ 数据源       │                                             │
│              │                                             │
│ 同步记录     │                                             │
│              │                                             │
│ 设置         │                                             │
│              │                                             │
│ 帮助与诊断   │                                             │
│              │                                             │
├──────────────┴─────────────────────────────────────────────┤
│ 3 个数据源 · 最近同步 2 分钟前 · AI 智能整理正常          │
└────────────────────────────────────────────────────────────┘
```

---

# 43. 窗口尺寸

推荐：

```text
默认：
1280 × 800

最小：
1024 × 680

Sidebar：
220～240 px

TopBar：
52～56 px
```

---

# 44. Knowledge 页面

最终目标：

```text
AIKS
└── 知识库
```

而不是：

```text
独立 AIKS Knowledge
```

---

# 45. Knowledge 实现优先级

## 优先方案

主窗口内嵌：

```text
SiYuan WebView
```

用户始终只有一个窗口。

---

## 过渡方案

如果 Tauri 内嵌 WebView 风险较高：

```text
点击知识库
↓
按需打开 AIKS - 知识库
```

但：

```text
启动时不自动创建

只第一次点击创建

以后复用

关闭不退出 AIKS
```

---

# 46. 概览页重新设计

不要显示：

```text
PID

Port

Runtime
```

首页展示：

```text
660
历史对话

3
已连接数据源

12
精炼知识

8
今日新增

正常
同步状态
```

---

# 47. 最近知识

首页：

```text
最近知识

AIKS Embedded SiYuan Runtime 修复
故障排查 · AIKS · 刚刚

vLLM Knowledge Extractor 设计
设计 · AIKS · 10 分钟前

EF Core 嵌套查询方案
实现 · DataOcean · 昨天
```

---

# 48. 首页 AI 状态

显示：

```text
AI 智能整理
● 正常
```

异常：

```text
AI 智能整理
● 暂时不可用
```

Raw Session 同步仍显示：

```text
正常
```

不要让用户误认为全部功能不可用。

---

# 49. 数据源页面

```text
OpenCode

已连接

518 条历史对话

精炼知识：103

最近同步：刚刚

[立即同步]
[查看会话]
```

---

# 50. Claude 未安装

不要：

```text
Error
```

而显示：

```text
Claude Code

未检测到

安装后 AIKS 会自动识别。
```

---

# 51. 一键全部同步

顶部：

```text
[全部同步]
```

结果：

```text
新增：12
更新：3
无变化：645

等待智能整理：8
```

---

# 52. 首次启动

首次安装：

```text
正在初始化 AIKS

✓ 知识引擎已启动

✓ 已发现 OpenCode
  518 条

✓ 已发现 Codex
  134 条

✓ 已发现 Gemini
  8 条

正在导入历史记录...
237 / 660
```

---

# 53. 首次 AI 整理不能阻塞

历史 Session 660 条。

不要安装后一次性全部用 AI 整理完才进入 UI。

推荐：

```text
Session Import
优先完成
```

AI Extract：

```text
后台逐步执行
```

例如：

```text
已导入 660

正在智能整理：
38 / 660
```

---

# 54. 历史知识整理策略

第一版不建议：

```text
全部 660 Session 无条件整理
```

先运行：

```text
Knowledge Score
```

例如：

```text
660 Sessions

↓ worth_extracting

210 Knowledge Candidates

↓ Extraction

210 Knowledge Documents
```

降低模型调用量。

---

# 55. Knowledge 页面视图

建议顶部：

```text
全部
最近
项目
类型
来源
标签
收藏
```

---

# 56. Project View

项目视图非常重要。

例如：

```text
AIKS
42 条知识

DataOcean
137 条

数据质量评测
96 条

字幕工具
58 条
```

项目名称来自：

```text
NormalizedSession.project_name
project_path
```

---

# 57. 原始 Session 与知识分开

Knowledge 页面默认看：

```text
精炼知识
```

用户需要时：

```text
查看原始会话
```

不要默认先面对 140 条 Message。

---

# 58. 搜索

顶部增加：

```text
搜索 AI 工作知识...
```

搜索范围：

```text
精炼知识

原始 Session

手工笔记
```

默认排序建议：

```text
精炼知识优先
```

---

# 59. 搜索结果

例如：

```text
AIKS Embedded SiYuan Runtime 修复

故障排查
AIKS
2026-09-12

修复了 SiYuan Runtime 版本锁定、
PowerShell Parser 和 Tauri 打包问题。

来源：
OpenCode
```

---

# 60. 收藏

精炼知识支持：

```text
★ 收藏
```

利用 SiYuan：

```text
Attribute
Tag
Bookmark
```

实现。

不要重新做复杂收藏系统。

---

# 61. 同步记录

业务视图：

```text
OpenCode

新增：3
更新：1
无变化：514

智能整理：
成功 2
跳过 1
```

---

# 62. AI 整理记录

展开：

```text
AIKS Runtime 修复
SUCCESS

模型：
Qwen3.8-27B

Knowledge Score：
0.91

类型：
troubleshooting
```

---

# 63. 技术日志独立

不要混：

```text
同步记录
```

和：

```text
技术日志
```

技术日志放：

```text
帮助与诊断
```

---

# 64. 设置页

普通设置：

```text
常规

开机启动

关闭后驻留后台

自动同步


同步

实时监听

定时扫描


AI 智能整理

启用智能整理

自动整理新会话

自动生成标签

自动提取问题

自动提取决策


内容

保存 Tool Call

保存 Tool Result

Secret Sanitizer
```

---

# 65. 高级设置

折叠：

```text
知识引擎

SiYuan 3.8.3


AI 服务

http://10.10.23.16:18000/v1

Qwen3.8-27B


State DB

C:\Users\...\aiks.db
```

---

# 66. 帮助与诊断

页面：

```text
AIKS 状态
✓ 正常

知识引擎
✓ 正常

OpenCode
✓ 正常

Codex
✓ 正常

Gemini
✓ 正常

AI 智能整理
✓ Qwen3.8-27B
```

操作：

```text
[重新检测]

[测试 AI]

[重新启动知识引擎]

[导出诊断包]

[打开日志]
```

---

# 67. 导出诊断包

生成：

```text
AIKS-Diagnostics-20260912.zip
```

包含：

```text
AIKS Version

OS

Source detection

Runtime version

AI model health

Last sync status

Knowledge extraction summary

Last 500 log lines
```

禁止包含：

```text
完整聊天正文

API Key

密码

Token

个人知识正文
```

---

# 68. 托盘菜单

```text
打开 AIKS

打开知识库

立即同步

暂停同步

────────

状态：正常

AI 整理：正常

────────

退出
```

---

# 69. 关闭主窗口

默认：

```text
关闭
↓
驻留后台
```

第一次提示：

```text
AIKS 将继续在后台同步和整理 AI 工作记录。
```

---

# 70. 页面视觉设计

公司内部工具：

```text
低饱和
高信息密度
简洁
稳定
```

不要：

```text
大面积渐变

营销 Banner

过度动画

过度圆角
```

---

# 71. Card

建议：

```text
圆角 6～8px

弱阴影

边框优先

8px spacing system
```

---

# 72. 首页 Card 数量

最多：

```text
4 个核心指标
```

不要做成十几个 Dashboard Card。

---

# 73. 页面名称企业化

避免：

```text
Provider

Runtime

Kernel

Sink
```

替换：

```text
数据源

知识引擎

同步

智能整理
```

---

# 74. 错误提示

不要显示：

```text
ECONNREFUSED

Kernel exit code 21
```

显示：

```text
AI 智能整理暂时不可用

无法连接公司内部 AI 服务。

原始会话仍会正常保存。

[重试]
[查看诊断]
```

---

# 75. AI Model 故障降级

模型不可用：

```text
OpenCode
↓
Session Sync
↓
Raw Session 保存
```

照常。

只显示：

```text
智能整理等待恢复
```

恢复后：

```text
自动继续 Queue
```

---

# 76. AI 模型并发

第一版：

```text
max_concurrent_extractions = 1
```

或者：

```text
2
```

不要默认过高。

避免和其他公司服务抢 GPU。

---

# 77. AI 使用统计

可以记录：

```text
request_count

success_count

failed_count

input_tokens

output_tokens

latency_ms
```

但第一版不需要复杂计费系统。

---

# 78. Prompt 管理

Prompts 不应散落代码。

例如：

```text
resources/prompts/
│
├── chunk-summary-v1.txt
├── knowledge-extractor-v1.txt
└── knowledge-classifier-v1.txt
```

或者 Rust 内嵌资源。

---

# 79. Prompt 核心要求

System Prompt 必须明确：

```text
你是企业研发工作知识整理器。

目标不是简单摘要，而是将 AI 编程会话转换为可复用的工程知识。

优先保留：

- 问题
- 根因
- 最终解决方案
- 设计决策
- 关键代码
- 命令
- 文件
- API
- 版本
- 配置
- 验证结果

压缩：

- 重复内容
- 无效尝试
- 冗余日志
- 机械性 Tool Output

禁止编造不存在的事实。

输出必须符合指定 JSON Schema。
```

---

# 80. JSON 校验

LLM Response：

```text
JSON parse
↓
Schema validation
```

失败：

```text
最多修复一次
```

可以再请求：

```text
请仅修复以下 JSON，使其满足 Schema。
```

仍失败：

```text
FAILED
```

不要保存错误 Markdown。

---

# 81. 自动知识更新

Session 后续有新消息：

```text
source_content_hash changed
```

现有 Knowledge：

```text
STALE
```

Debounce 后：

```text
重新 Extract
```

更新同一个 Knowledge Document。

不要创建重复知识文档。

---

# 82. Conflict

如果用户手工编辑了 AI Knowledge：

不得静默覆盖。

推荐：

```text
AIKS Managed Section
```

或者属性：

```text
custom-aiks-managed=true
```

如果发现用户修改：

```text
CONFLICT
```

第一版可以：

```text
生成新版 AI 建议
```

而不是覆盖。

---

# 83. 更推荐的 Knowledge 文档结构

为了允许用户编辑：

```text
# 标题

## AI 摘要
AIKS Managed

## 问题
AIKS Managed

## 解决方案
AIKS Managed

## 用户补充
User Managed
```

后续 AI Update：

只更新：

```text
AIKS Managed
```

区域。

---

# 84. 数据目录

继续：

```text
%LOCALAPPDATA%\AIKnowledgeSync\
```

建议：

```text
data/
  aiks.db
  archive/

siyuan/
  workspace/

logs/

config/

diagnostics/
```

---

# 85. 不要重建已有能力

明确禁止实现：

```text
新的向量数据库

新的全文搜索引擎

新的 Markdown Editor

新的 RAG Engine

新的知识库后端
```

继续使用 SiYuan。

---

# 86. 本轮建议新增页面

新增：

```text
KnowledgePage.tsx

DiagnosticsPage.tsx
```

优化：

```text
OverviewPage.tsx

SourcesPage.tsx

SyncPage.tsx

SettingsPage.tsx

Sidebar.tsx
```

---

# 87. Rust 改动范围

建议：

```text
crates/aiks-core/src/
│
├── ai/
│   ├── client.rs
│   ├── extractor.rs
│   ├── chunker.rs
│   ├── schema.rs
│   └── prompts.rs
│
├── knowledge/
│   ├── model.rs
│   ├── renderer.rs
│   └── service.rs
│
└── storage/
```

---

# 88. Desktop Commands

新增 Tauri Commands：

```text
get_ai_status

test_ai_connection

get_extraction_queue

retry_extraction

extract_session_now

get_knowledge_stats

get_recent_knowledge
```

---

# 89. Initial Sync

最终启动：

```text
AIKS Start
   ↓
Embedded SiYuan
   ↓
Provider Scan
   ↓
Raw Session Sync
   ↓
UI Ready
   ↓
Background Extraction Queue
```

不要：

```text
等 AI 全部整理完成
↓
UI 才打开
```

---

# 90. 第一阶段实施范围 P0

公司内部试用前必须：

```text
单窗口

Knowledge 不自动弹出

知识库入口

AI Provider

Qwen3.8-27B 接入

Knowledge Extractor

Knowledge Score

Structured JSON Output

Raw + Knowledge 双层存储

AI Queue

模型失败降级

项目视图

设置页 AI 状态

诊断页

导出诊断包
```

---

# 91. 第二阶段 P1

试用后：

```text
顶部全局搜索

收藏

最近查看

企业策略

Prompt Version 管理

历史重新整理

内部自动更新
```

---

# 92. 第三阶段 P2

用户规模增加以后再做：

```text
团队知识共享

中央知识服务

SSO

企业权限

跨设备同步

团队 Knowledge Graph

知识推荐

自动周报
```

---

# 93. 本轮明确不做

暂时不要：

```text
多 Agent

复杂 RAG

知识图谱

自动写 Wiki

自动生成团队文档

自动修改代码

团队聊天机器人
```

先把：

```text
Session
↓
High Quality Knowledge
```

做好。

---

# 94. 核心验收场景

选择已有：

```text
OpenCode ses_f717
```

原始：

```text
约 140 Messages
```

要求：

```text
10 AI Sessions
```

保留完整 Session。

同时：

```text
20 Knowledge
```

生成：

```text
项目骨架实现与关键设计决策
```

内容必须：

```text
明显短于原始会话

能看懂最终方案

包含重要文件

包含关键设计决策

包含问题和解决方案

能跳回原始 Session
```

---

# 95. AI 模型验收

测试：

```text
http://10.10.23.16:18000/v1
```

模型：

```text
Qwen3.8-27B
```

必须：

```text
连接成功

结构化 JSON 成功

中文正常

代码内容正常

Session 过长能 Chunk

模型失败不影响 Raw Sync
```

---

# 96. UI 验收

必须：

```text
[ ] 启动只出现一个 AIKS

[ ] Knowledge 不自动弹出独立窗口

[ ] 首页不显示 Kernel / Port

[ ] 有知识库入口

[ ] 数据源页面可看到 OpenCode / Codex / Gemini

[ ] 有“全部同步”

[ ] 首页有最近知识

[ ] 有项目视图

[ ] AI 状态可见

[ ] AI 异常不影响 Session 同步

[ ] 可以手动“立即整理”
```

---

# 97. 企业内用验收

员工安装：

```text
AIKS-Setup-x64.exe
```

启动：

```text
AIKS
```

不需要：

```text
SiYuan

PowerShell

Token

Port

模型 URL

Provider 配置
```

自动：

```text
发现 AI 工具
↓
同步历史
↓
整理知识
↓
可以搜索
```

---

# 98. 最终用户体验

理想状态：

```text
员工今天使用 OpenCode 编程

↓ 10 分钟后

AIKS 自动捕获

↓

公司内部 Qwen3.8-27B 自动整理

↓

生成：

问题
根因
方案
命令
文件
决策

↓

进入个人知识库

↓

一个月以后搜索：

“SiYuan Runtime”

↓

直接找到过去解决方案
```

---

# 99. 最终产品结构

```text
AIKS Desktop
│
├── 数据采集
│   ├── OpenCode
│   ├── Codex
│   ├── Gemini
│   └── Claude
│
├── Raw Session
│
├── AI Knowledge Extractor
│   └── Qwen3.8-27B
│       ├── 去噪
│       ├── 分类
│       ├── 摘要
│       ├── 标签
│       ├── 决策
│       ├── 问题
│       └── 解决方案
│
├── Knowledge
│   ├── Projects
│   ├── Troubleshooting
│   ├── Implementation
│   ├── Design
│   └── Research
│
└── Embedded SiYuan
    ├── Storage
    ├── Search
    ├── Notes
    └── Editing
```

---

# 100. 给 Coding Agent 的执行要求

请基于当前 AIKS V2 项目实施本方案。

不得重新实现已有：

```text
Provider

Canonical Model

Sync Engine

Watcher

SiYuan Runtime
```

重点实现：

```text
1. 单窗口产品体验

2. Knowledge 页面

3. OpenAI-compatible AI Client

4. 默认接入：

   Base URL:
   http://10.10.23.16:18000/v1

   Model:
   Qwen3.8-27B

5. Knowledge Extractor

6. Structured JSON Schema

7. Long Session Chunk / Reduce

8. Knowledge Score

9. Background Extraction Queue

10. Raw Session + Knowledge 双层结构

11. AI Failure Graceful Degradation

12. Project View

13. AI Settings

14. Diagnostics
```

实现结束后必须真实验证：

```text
OpenCode ses_f717
↓
Raw Session 正常
↓
Qwen3.8-27B 正常调用
↓
Knowledge JSON 正常
↓
Knowledge Markdown 正常
↓
SiYuan 正常写入
↓
Knowledge 页面可查看
↓
可跳回 Raw Session
```

同时执行：

```text
cargo check --workspace

cargo test --workspace

npm run build

Tauri dev

Tauri release build
```

不能只以：

```text
cargo test passed
```

作为完成依据。

必须真实验证：

```text
Desktop UI
+
vLLM
+
SiYuan
+
真实 OpenCode Session
```

整个链路。

---

# 101. 最终目标

本轮完成后，AIKS 应从：

```text
AI Session Collector
```

升级为：

```text
AI Work Knowledge System
```

最终产品体验：

```text
使用 AI
   ↓
AIKS 自动记录
   ↓
公司模型自动整理
   ↓
形成知识
   ↓
持续搜索和复用
```

这才是适合公司内部长期推广使用的 AIKS。