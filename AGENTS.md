# CmdFlow — Agent 入口

> **这是什么**：本地命令编排工具（"可视化版 shell history"）。
> Tauri 2 桌面应用，Rust 后端 + React 18 前端，单二进制 + SQLite。
> **当前状态**：Phase 0–9 全部完成，v0.2.3 已构建（含 Windows / Linux / macOS-ARM 资产）。

## 快速开始

```powershell
$env:LC_ALL = "C.UTF-8"                # PowerShell GBK locale, git commit 需要
cd "C:/Users/19114/.minimax-agent-cn/projects/CmdFlow"
pnpm install                              # esbuild postinstall 默认被屏蔽
pnpm rebuild esbuild                      # 手动批准 esbuild
pnpm tauri:dev                             # 启动 dev (Vite + Tauri)
```

打开 CmdFlow 任意工作流编辑器，**鼠标 hover 节点**应变 grab 手型，**点击节点**应触发属性面板。

## 项目结构

```
src/                            React 前端
├── components/
│   ├── layout/      MainLayout / Sidebar / StatusBar
│   ├── palette/     CommandPalette (popup 模式 fallback)
│   ├── workflow/    NodePanel / PropertyPanel / ReactFlowLazy / WorkflowEditor
│   ├── forms/       CommandEditor
│   └── ui/          通用组件 (Button / Input / Dialog / Card)
├── pages/           9 个页面 (Library / Workflows / Templates / Runner / History
│                    / Schedules / Plugins / Settings / Palette)
├── lib/             tauri.ts (API 客户端) / utils.ts / paramHistory.ts
├── stores/          Zustand (theme)
├── styles/          globals.css (Tailwind 入口)
├── main.tsx         createRoot + StrictMode
└── router.tsx       createHashRouter (9 个 route)

src-tauri/                     Rust 后端
├── src/
│   ├── lib.rs                 Tauri app 入口 (register commands + setup)
│   ├── main.rs                bin 入口 (调用 lib)
│   ├── state.rs                AppState (DB / scheduler / executor)
│   ├── error.rs                AppError + AppResult
│   ├── commands/               14 个 Tauri command 模块
│   │   ├── library.rs           命令库 CRUD
│   │   ├── category.rs          分类 CRUD + 拖拽排序
│   │   ├── favorite.rs          收藏
│   │   ├── workflow.rs          工作流 CRUD + run/cancel/validate
│   │   ├── execution.rs         执行历史
│   │   ├── history.rs           HistorySummary
│   │   ├── schedule.rs          cron 调度
│   │   ├── templates.rs         6 个内置 YAML 模板
│   │   ├── yaml_io.rs           YAML 导入导出
│   │   ├── plugin.rs            JS + WASM 插件运行时
│   │   ├── window.rs            多窗口 (palette 独立窗口)
│   │   └── system.rs
│   ├── nodes/                  9 个节点类型
│   │   ├── cmd.rs script.rs http.rs ai.rs file.rs delay.rs
│   │   ├── condition.rs loop_node.rs subworkflow.rs
│   │   ├── node.rs              trait Node + NodeRegistry
│   │   └── registry.rs
│   ├── core/                   executor / dag / scheduler / events / interpolation
│   ├── storage/
│   │   ├── db.rs                r2d2 连接池 + open_pool_memory (Linux 用 temp file)
│   │   ├── migrations.rs        MIGRATIONS 数组 (按顺序执行)
│   │   ├── models.rs            13 个 sqlite struct
│   │   └── seed.rs              首次启动种子数据 (4 分类 + 7 命令 + 2 工作流)
│   ├── js_runtime.rs            Boa engine 包装
│   └── wasm_runtime.rs          wasmtime 24 包装
├── templates/                   6 个 YAML 内置工作流模板
├── migrations/                  4 个 SQL schema 迁移 (按文件名顺序)
├── icons/                       多平台图标 (icon.ico / icon.icns / *.png)
└── tauri.conf.json              Tauri 配置 (含 app.macOSPrivateApi = true)

migrations/                      跟 src-tauri/migrations/ 同步, 通过 include_str! 引用
.github/workflows/
├── ci.yml                       门禁 (PR + push main): tsc + vite + cargo fmt + clippy + test
└── release.yml                  tag v*.*.* 触发: 4 平台矩阵构建
```

## 开发命令

| 命令 | 用途 |
|---|---|
| `pnpm tauri:dev` | 完整 dev (Vite + Tauri WebView) |
| `pnpm dev` | 只跑 Vite, 浏览器访问 http://localhost:1420 |
| `pnpm build` | 前端生产构建 (tsc + vite) |
| `pnpm tauri:build` | 完整打包 (产出 .msi / .exe / .deb / .dmg) |
| `pnpm exec tsc --noEmit` | 类型检查 |
| `cargo test --lib` | Rust 单元测试 (35 个) |
| `cargo fmt --all -- --check` | CI 门禁: rustfmt 格式 |
| `cargo clippy --all-targets --no-deps -- -D warnings` | CI 门禁: clippy 零警告 |

## 添加节点类型

1. `src-tauri/src/nodes/<name>.rs` 实现 `Node` trait (从 `node.rs` 看接口)
2. `src-tauri/src/nodes/mod.rs` 注册
3. `src-tauri/src/nodes/registry.rs` 添加默认配置
4. `src/components/workflow/NodePanel.tsx` 加 ICONS 映射 (lucide-react)
5. `src/components/workflow/PropertyPanel.tsx` 的 SchemaForm 自动渲染 config_schema
6. 写单测: `cargo test --lib <name>`

## 添加工作流模板

在 `src-tauri/templates/<name>.yaml` 写 YAML, 然后在
`src-tauri/src/storage/seed.rs` 注册 (idempotent, 首次启动自动注入)。

## 数据库迁移

1. `migrations/000N_xxx.sql` 写 SQL
2. `src-tauri/src/storage/migrations.rs` 的 `MIGRATIONS` 数组添加 `(name, sql)`
3. SQL 通过 `include_str!` 嵌入二进制

⚠️ **坑**: 0001 和 0004 都定义了 `idx_commands_category` 索引, 列不同。
0004 改名 `idx_commands_category_id`。新增迁移时检查已有索引名。

## CI 流程

- **ci.yml** (PR + push main): `frontend` (tsc + vite) + `backend` (fmt + clippy + test) 都过才 merge
- **release.yml** (push tag `v*.*.*`): 4 平台矩阵构建
  - `windows-latest` → msi + nsis
  - `ubuntu-22.04` → deb + rpm + AppImage
  - `macos-latest` → aarch64 dmg + .app (✅ 正常)
  - `macos-13` → x86_64 dmg (**❌ 永远拿不到 runner, 不要等**)

## 常见坑

| 坑 | 现象 | 解决 |
|---|---|---|
| `WebviewWindowBuilder::transparent(true)` 编译失败 | macOS 是私有 API | `Cargo.toml` 加 `features = ["macos-private-api"]`, `tauri.conf.json` 加 `app.macOSPrivateApi: true` |
| 节点 hover grab 但 click 不响应 | reactflow 11 d3-drag 的 mouseup `preventDefault` 阻止 click dispatch | **必须用 @xyflow/react v12+**, v11 修不了 |
| macOS runner 卡 queue 11+ 小时 | GitHub Actions 紧缺 | 接受只发 3 平台 (Win/Linux/macOS-ARM), 取消 macos-13 job |
| `icons/icon.ico` not found | tauri-build 找不到 Windows 图标 | `pnpm exec tauri icon` 从源图标生成所有平台 |
| React 18 + StrictMode double mount | useEffect 跑两次 | dev mode 才发生, 生产构建没事 |
| esbuild postinstall 警告 | `ERR_PNPM_IGNORED_BUILDS` | CI 必须 `pnpm rebuild esbuild` |
| PowerShell GBK locale, git commit 乱码 | 中文 commit message 截断 | `$env:LC_ALL = "C.UTF-8"` 必备 |

## 关键设计决策

- **dnd-kit** 拖拽改分类 (react-dnd hooks-first 更轻)
- **Boa 0.20** JS 插件 (pure Rust, 无 v8/wasm 依赖) — 替代 QuickJS
- **wasmtime 24** WASM 插件, `default-features = false, features = ["runtime", "std", "cranelift"]` 避开 cache 冲突
- **rusqlite bundled** — 不依赖系统 SQLite, 跨平台一致
- **@xyflow/react v12** — v11 的 d3-drag click bug 修不了
- **react-router-dom 6 createHashRouter** — Tauri WebView 用 hash 路由避免 server 路由问题
- **tauri.conf.json app.macOSPrivateApi** — 启动器独立窗口需要透明 + 无边框

## 测试

- **35/35 单元测试** (base 18 / templates 3 / wasm 2 / ai 6 / seed 6)
- 跑测试: `cd src-tauri && cargo test --lib`
- 加测试: 在 `src-tauri/src/<module>.rs` 同目录加 `#[cfg(test)] mod tests`

## Git 流程

- 远程: `git@github.com:ZhSMM/CmdFlow.git` (SSH 走 ~/.ssh/)
- HTTPS 在国内 timeout, **永远用 SSH**
- Commit: 中文 message 必须 UTF-8 (PowerShell 需 `LC_ALL=C.UTF-8`)
- Tag: `git tag -a vX.Y.Z -m "..."`, `git push origin vX.Y.Z` 触发 release.yml
- Release 默认 draft (`releaseDraft: true`), 手动 `gh release edit --draft=false` 发布
- Version bump 要同步: `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` + `package.json` + `RELEASE_NOTES.md`

## 调试技巧

- **节点拖动/click 不工作**: 升级到 v12
- **节点位置/handle 异常**: 看 `.react-flow__node` 的 `data-id` 和 `pointer-events` (CSS specificity 问题)
- **migration 报错**: `src-tauri/src/storage/migrations.rs` 确认新 SQL 已加入 `MIGRATIONS` 数组
- **WebView 看不到 devtools**: 用户侧 F12 直接打开 (Tauri 2 默认开启 devtools)
- **macOS build 失败**: 看 release.yml 日志, 多半是 `macos-private-api` 没启用
