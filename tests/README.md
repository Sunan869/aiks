# Tests

建议测试分层：

```text
unit/
provider_golden/
renderer_golden/
integration/siyuan/
performance/
```

Provider Golden Test 至少验证：

- session id
- title
- project/cwd
- message count
- role distribution
- text content
- tool calls
- tool results
- timestamp

SiYuan Integration Test 必须针对测试 Notebook，不要污染用户真实知识库。
