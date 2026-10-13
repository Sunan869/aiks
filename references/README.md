# Upstream Reference Repositories

该目录用于 Clone 上游参考项目，默认不提交进 AIKS 自身 Git 仓库。

建议目录：

```text
references/
├── aicoder-session-viewer/
├── cc-switch/
├── ccusage/
├── mnemos/
├── codex/
├── gemini-cli/
├── opencode/
├── claude-code/
└── siyuan/
```

建议命令：

```bash
git clone https://github.com/seastart/aicoder-session-viewer.git aicoder-session-viewer
git clone https://github.com/farion1231/cc-switch.git cc-switch
git clone https://github.com/ccusage/ccusage.git ccusage
git clone https://github.com/mnemos-dev/mnemos.git mnemos
git clone https://github.com/openai/codex.git codex
git clone https://github.com/google-gemini/gemini-cli.git gemini-cli
git clone https://github.com/anomalyco/opencode.git opencode
git clone https://github.com/anthropics/claude-code.git claude-code
git clone https://github.com/siyuan-note/siyuan.git siyuan
```

Clone 后必须记录每个项目：

```bash
git rev-parse HEAD
```

并填写到：

```text
docs/reference-analysis/
THIRD_PARTY_NOTICES.md
```

不要只记录 main/master 分支名。
