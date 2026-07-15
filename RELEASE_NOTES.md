# CmdFlow v0.2.1

## 修复 (v0.2.1)

- **启动器快捷键改回 Ctrl+Alt+P** — 原 `Cmd+Shift+Space` 在 Windows 上跟 Windows Search / 某些中文输入法冲突，OS 直接拒绝注册。改成 `Ctrl+Alt+P` (P for Palette)，通用且几乎不冲突
- **CI 收紧** — clippy 门禁从 warn 升级为 `deny warnings`，再有任何 warning 都进不来 main
- **clippy 全清** — 33 个历史 warning 全部清理：删除未用 import/变量、消除 dead code、while-let 化、merge match 简化、折叠可合并的 if let 等

## v0.2.0 回顾

Phase 8 + Phase 9 重大更新：插件体系、工作流、模板市场、多窗口、AI 流式。

## 新增 (Phase 9)

- **拖拽改分类** — dnd-kit 替换原下拉选择, 命令卡片可拖到任意分类
- **节点模板市场** — 6 个内置工作流 (git-deploy / docker-build / log-cleanup / http-data-pipeline / daily-backup / http-healthcheck) 一键导入
- **WASM 插件** — wasmtime 24.x 嵌入, .wasm 插件可独立执行 (合约: alloc/dealloc/run)
- **多窗口启动器** — Ctrl+Alt+P 拉独立窗口 (透明 + 置顶 + 无边框), popup 模式保留为 fallback
- **AI 流式输出** — SSE 逐 token 推送, UI 可做 typewriter
- **AI 工具调用** — OpenAI 格式 tools, 工具名映射命令库, 自动执行 + 反馈 LLM

## Phase 8 (本版本并入)

- **JS 插件运行时** — Boa engine 0.20 (pure Rust), 沙箱执行 + 5s 超时
- **树形 LibraryPage** — 多级分类, 收藏侧栏
- **YAML 导入导出** — 工作流可导出 .yaml, 跨环境迁移

## 节点类型 (9)

`cmd` `script` `http` `ai` `file` `delay` `condition` `loop` `subworkflow`

## 插件格式 (2)

- **JS** — manifest.json + index.js, 导出 `run` 函数
- **WASM** — manifest.json + plugin.wasm, 导出 `memory` + `alloc/dealloc/run`

## 安装

下载对应版本：
- **MSI** (推荐): Windows Installer
- **EXE** (NSIS): 单文件安装器
- **.deb / .AppImage** (Linux)
- **.dmg** (macOS)

系统要求: Windows 10/11 + WebView2 Runtime / 现代 Linux + webkit2gtk-4.1 / macOS 11+

## 技术栈

Tauri 2 · Rust · React 18 · TypeScript · SQLite · ReactFlow · xterm.js

## 文档

- [设计文档](https://github.com/ZhSMM/CmdFlow/blob/main/DESIGN.md)
- [用户指南](https://github.com/ZhSMM/CmdFlow/blob/main/docs/USER_GUIDE.md)
- [README](https://github.com/ZhSMM/CmdFlow#readme)

## 测试

14/14 单元测试通过 (interpolation / blacklist / scheduler)
