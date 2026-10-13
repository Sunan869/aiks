# AIKS 应用图标实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 用已确认的 A「同步核心」设计替换 AIKS 桌面应用的占位图标，并生成 Tauri 所需的平台资源。

**架构：** 在图标目录保留一份 SVG 母版，使用 Tauri CLI 在隔离输出目录生成 PNG、ICO 和 ICNS，再将配置需要的资源复制回正式目录。托盘关闭模板模式以保留青蓝紫配色；验证脚本直接检查图片尺寸、文件签名与 Tauri 配置引用。

**技术栈：** SVG、Tauri CLI 2、PowerShell、System.Drawing、Vite/TypeScript。

---

## 文件结构

- 创建：`apps/aiks-desktop/src-tauri/icons/icon.svg` — 可维护的 1024×1024 矢量母版。
- 替换：`apps/aiks-desktop/src-tauri/icons/32x32.png` — Windows/桌面小图标。
- 替换：`apps/aiks-desktop/src-tauri/icons/128x128.png` — 标准 PNG 图标。
- 替换：`apps/aiks-desktop/src-tauri/icons/128x128@2x.png` — 256 px 高分屏图标。
- 替换：`apps/aiks-desktop/src-tauri/icons/icon.png` — 托盘和通用高分辨率图标。
- 替换：`apps/aiks-desktop/src-tauri/icons/icon.ico` — Windows 多尺寸图标。
- 创建：`apps/aiks-desktop/src-tauri/icons/icon.icns` — macOS 图标。
- 修改：`apps/aiks-desktop/src-tauri/tauri.conf.json` — 关闭彩色托盘图标的 template 模式。

### 任务 1：生成并接入「同步核心」图标

**文件：** 上述图标资源和 Tauri 配置。

- [ ] **步骤 1：记录现有资源失败基线**

运行：

```powershell
Add-Type -AssemblyName System.Drawing
Get-ChildItem apps/aiks-desktop/src-tauri/icons/*.png | ForEach-Object {
  $image = [System.Drawing.Image]::FromFile($_.FullName)
  [pscustomobject]@{ Name = $_.Name; Width = $image.Width; Height = $image.Height }
  $image.Dispose()
}
Test-Path apps/aiks-desktop/src-tauri/icons/icon.icns
```

预期：四个 PNG 都是 1×1，ICNS 返回 False，证明当前资源不满足规格。

- [ ] **步骤 2：创建 SVG 母版**

用 `viewBox="0 0 1024 1024"` 绘制圆角深蓝底、两段同步环、箭头和中央知识节点。背景渐变为 `#182956` → `#0A1024`，同步环为 `#69F3FF` → `#478CFF` → `#986BFF`，中央节点为 `#E9FBFF`。所有坐标使用整数并留出约 8% 安全边距，不加入文字和滤镜，保证 rasterizer 输出稳定。

- [ ] **步骤 3：在隔离目录生成平台资源**

运行：

```powershell
Set-Location apps/aiks-desktop
npx tauri icon src-tauri/icons/icon.svg --output src-tauri/.icon-output
```

预期：命令成功，隔离目录包含 32×32、128×128、256×256、通用 PNG、ICO 和 ICNS。生成失败时先修正 SVG，不覆盖正式文件。

- [ ] **步骤 4：复制经过确认的产物并更新配置**

将隔离目录中的 `32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.png`、`icon.ico`、`icon.icns` 复制到正式 icons 目录。把 `tauri.conf.json` 中 `app.trayIcon.iconAsTemplate` 从 `true` 改为 `false`，使 Windows/Linux 托盘保留彩色设计。

- [ ] **步骤 5：验证资源结构和视觉效果**

运行尺寸检查并要求：32×32、128×128、256×256 和高分辨率 icon.png 的尺寸分别正确；`icon.ico`、`icon.icns` 非空且所有 Tauri 配置引用存在。用图片查看工具检查 icon.png，并生成 16/32/64 px 联系表，确认同步环和中央节点在小尺寸仍可辨认。

- [ ] **步骤 6：验证前端与 Tauri 配置**

运行：

```powershell
Set-Location apps/aiks-desktop
npm run build
npx tauri info
```

预期：两条命令退出码为 0，Tauri 不报告缺失图标。若当前工作区其他用户改动造成独立构建失败，记录原始错误并单独验证图标文件与配置。

- [ ] **步骤 7：清理生成目录并检查差异**

解析并确认 `apps/aiks-desktop/src-tauri/.icon-output` 的绝对路径位于项目内，再删除该生成目录。运行 `git diff --check`，确保仅图标母版、六个资源和 `tauri.conf.json` 属于本任务；不暂存其他已有修改。

- [ ] **步骤 8：提交图标变更**

```powershell
git add -- apps/aiks-desktop/src-tauri/icons/icon.svg apps/aiks-desktop/src-tauri/icons/32x32.png apps/aiks-desktop/src-tauri/icons/128x128.png apps/aiks-desktop/src-tauri/icons/128x128@2x.png apps/aiks-desktop/src-tauri/icons/icon.png apps/aiks-desktop/src-tauri/icons/icon.ico apps/aiks-desktop/src-tauri/icons/icon.icns apps/aiks-desktop/src-tauri/tauri.conf.json
git commit -m "feat(desktop): replace AIKS app icon"
```

预期：提交只包含图标及其消费配置，用户现有未提交代码保持原状。
