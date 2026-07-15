# CmdFlow v0.2.3

## 修复 (v0.2.3)

- **macOS 透明窗口支持** — `WebviewWindowBuilder::transparent` 在 macOS 上是私有 API，
  Tauri 2 默认不暴露。开启 `macos-private-api` feature + `app.macOSPrivateApi: true` 后，
  启动器独立窗口在 macOS 上也能正常透明渲染。Windows / Linux 行为不变。

## 修复 (v0.2.2)

> v0.2.2 的 release 因 macOS 编译失败被撤回, 实际修复已合并到 v0.2.3.

- **补迁移 0004** — `categories` / `favorites` 表的迁移文件之前没注册到 `MIGRATIONS`
  数组, 旧数据库升级到 v0.2.x 时会报 `no such table: favorites`. 现在纳入迁移链路.
- **首次启动种子数据** — 新建空库时自动注入 4 个分类 + 7 个常用命令 (echo / ls / ping /
  读文件 / HTTP GET / Git 状态 / AI 问答) + 2 个示例工作流 (每日备份 / HTTP 健康检查),
  避免用户打开应用面对空库.
- **索引重命名** — `idx_commands_category` 在 0001 和 0004 各定义过一次 (列不同),
  改名为 `idx_commands_category_id` 避免冲突.
- **共享内存连接改用临时文件** — `open_pool_memory` 切换到 temp file 模式,
  规避 r2d2 在某些 Linux 平台上 `shared memory` 不可用的问题.

## 新增 (Phase 9)

- **拖拽改分类** — dnd-kit 替换原下拉选择, 命令卡片可拖到任意分类
- **节点模板市场** — 6 个内置工作流 (git-deploy / docker-build / log-cleanup /
  http-data-pipeline / daily-backup / http-healthcheck) 一键导入
- **WASM 插件** — wasmtime 24.x 嵌入, .wasm 插件可独立执行 (合约: alloc/dealloc/run)
- **多窗口启动器** — Ctrl+Alt+P 拉独立窗口 (透明 + 置顶 + 无边框), popup 模式保留为 fallback
- **AI 流式输出** — SSE 逐 token 推送, UI 可做 typewriter
- **AI 工具调用** — OpenAI 格式 tools, 工具名映射命令库, 自动执行 + 反馈 LLM

## Phase 8 (并入)

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

Tauri 2 · Rust · React 18 · TypeScript · SQLite · ReactFlow · xterm.js · dnd-kit

## 文档

- [设计文档](https://github.com/ZhSMM/CmdFlow/blob/main/DESIGN.md)
- [用户指南](https://github.com/ZhSMM/CmdFlow/blob/main/docs/USER_GUIDE.md)
- [README](https://github.com/ZhSMM/CmdFlow#readme)

## 测试

35/35 单元测试通过 (base 18 / templates 3 / wasm 2 / ai 6 / seed 6)
