# TheBrain

TheBrain 是一个本地优先的外置大脑桌面应用，目标是提供类似 Obsidian 的个人知识管理体验，并围绕可组织、可连接、可长期维护的本地知识库展开。

## 当前状态

项目当前处于 v0.2 基础闭环实现阶段。Tauri 2 + React + Vite TypeScript 桌面骨架已经扩展为可运行的 Vault 工作台，覆盖收集箱、Markdown、AI 整理队列、预算、移动审计/回滚、TODO/日程候选、便利贴和设置页。

已实现的能力属于本地优先基础实现：真实文件移动和回滚会落在用户选择的 Vault 内；MiMo provider 支持真实接口路径和 key 查找，同时在无 key、网络失败或 API 失败时返回可测试 fallback 状态，不伪造真实成功。

## 凭据与安全

真实 API Key 只允许存在于以下位置：

- 本机环境变量 `MIMO_API_KEY`
- gitignored 的 `.secrets/mimo_api_key.txt`
- 用户显式传入的运行时参数

真实 key 不进入 README、代码、测试、日志、SQLite seed、前端错误信息或提交内容。`.secrets/` 已加入 `.gitignore`。仓库提供 `.env.local.example` 作为占位示例，不包含真实 key。

## 当前架构

项目管理层：

- `AGENTS.md` 记录项目级协作规则、项目经理身份、功能开发前先规划并确认的要求。
- `.codex/project.md` 记录项目管理说明和 README 维护规则。
- `.codex/skills/mimo-audio-transcriber` 提供 MiMo v2.5 音频转文字工作流脚本，key 查找遵守本地凭据规则。

前端层：

- `src/App.tsx` 是生产级工作台 shell，包含总览、Markdown、AI 整理、TODO/日程、便利贴、设置页和状态侧栏。
- `src/api.ts` 封装 Tauri commands，并对缺失或失败命令提供 fallback 状态，避免 UI 崩溃。
- Markdown 预览使用 `marked` + `DOMPurify`，预览默认隐藏 YAML frontmatter。

Tauri/Rust 层：

- `src-tauri/src/lib.rs` 注册 dialog、single-instance、global-shortcut 插件，注册 v0.2 commands，并管理收集箱 watcher registry。
- `src-tauri/src/commands.rs` 暴露 Vault、Markdown、收集箱、ledger、队列、预算、MiMo、整理、移动/回滚、TODO/日程候选、便利贴、快捷键和窗口预热 commands。
- `src-tauri/src/services/index.rs` 管理 `.thebrain/index.sqlite` schema。
- `src-tauri/src/services/queue.rs` 管理文件监听状态、持久化队列、去重入口和单并发队列状态。
- `src-tauri/src/services/dedupe.rs` 使用调用方提供的 dedupe key/hash 记录去重命中，不额外引入 hash 策略。
- `src-tauri/src/services/budget.rs` 管理预算设置、暂停、重试上限、冷却参数和整数 cents 用量账本。
- `src-tauri/src/services/ai.rs` 接入 `mimo-v2.5` 文件内容抽取和 `mimo-v2.5-pro` 整理决策流程，并提供 mock/fallback。
- `src-tauri/src/services/movement.rs` 执行收集箱内文件移动、更新 ledger、记录 movement log，并提供真实回滚。
- `src-tauri/src/services/candidates.rs` 管理 TODO/日程候选，AI 只生成候选，用户确认后才写入确认状态。
- `src-tauri/src/services/sticky.rs` 管理便利贴持久化，并支持自动保存到 `000-收集箱` Markdown。
- `src-tauri/src/services/settings.rs` 管理整理范式、预算、快捷键、便利贴窗口和冲突策略设置。

Vault 结构：

```text
Vault/
  000-收集箱/
    收集箱-已整理.md
  .thebrain/
    index.sqlite
```

`.thebrain/index.sqlite` 当前包含：`vault_meta`、`file_index`、`ai_usage`、`audit_events`、`listener_state`、`queue_items`、`dedupe_records`、`budget_settings`、`budget_ledger`、`movement_log`、`action_candidates`、`sticky_notes`。

## 行为边界

- AI 自动整理只能移动 `000-收集箱/` 下的待整理文件。
- `000-收集箱/收集箱-已整理.md` 不能被 AI 移动。
- 所有 Vault 文件路径都通过相对路径规范化校验，拒绝绝对路径和 `..` 逃逸。
- 移动和回滚不会覆盖已有文件；目标冲突会返回错误状态。
- `收集箱-已整理.md` 使用相对 wikilink，例如 `[[../100-School/笔记.md]]`，映射历史归档项。
- embedding、AI prompt/response、token usage、移动历史、队列日志不写入 Markdown frontmatter，只进入 `.thebrain` 内部结构。
- 第一版整理范式固定为“文档管理 + 知识标签”，其他范式仅保留扩展占位。

## 已完成目标

- 初始化 Tauri 2 + React + Vite TypeScript 应用骨架。
- 实现 Vault 选择与 Vault 结构幂等初始化。
- 实现 Markdown 编辑、预览、保存、导出和 YAML frontmatter 过滤。
- 实现 `000-收集箱` 文件展示与 `收集箱-已整理.md` 历史映射。
- 实现 `.thebrain` SQLite 内部索引与 v0.2 基础 schema。
- 实现文件监听入口、持久化队列、去重、防循环和默认单并发状态。
- 接入 `mimo-v2.5` 文件内容抽取流程和 `mimo-v2.5-pro` 整理决策流程，包含 no-key/network/API fallback。
- 实现 AI 收集箱文件移动、ledger 更新、movement log 和真实回滚。
- 实现 TODO/日程候选确认流程。
- 实现预算账本、暂停开关、重试上限、冷却时间和预算耗尽状态。
- 实现便利贴持久化、自动保存到收集箱 Markdown、可配置全局快捷键和少量窗口预热。
- 实现生产级 UI shell、错误恢复提示、冲突提示占位和设置页。
- 固化 Rust 单元测试，覆盖队列、去重、预算、移动/回滚、候选确认、便利贴、Markdown 和路径安全。

## 未完成目标

- 对真实 MiMo API 进行长时间、多文件类型、带成本的实机联调。
- 进一步完善托盘菜单、窗口池回收和便签独立窗口前端路由。
- 扩展冲突解决为完整交互式策略，例如重命名建议、差异预览和批量处理。
- 补充端到端 GUI 自动化测试和真实文件监听长时间稳定性测试。
- 实现更复杂的整理模板切换和用户自定义规则。

## Test Plan

当前已通过的验证门槛：

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`

当前 Rust 单元测试覆盖：

- Vault 初始化幂等，不覆盖已有 ledger。
- 路径拒绝绝对路径、`..` 和 Vault 逃逸。
- Markdown frontmatter 保存、预览隐藏和导出过滤。
- ledger 相对 wikilink 解析。
- SQLite schema 创建。
- 队列监听状态、持久化队列和 dedupe key 去重。
- 预算 limits、暂停和耗尽状态。
- MiMo extract mock/fallback 文件读取路径。
- AI 移动只允许收集箱内文件，拒绝 ledger 和非收集箱文件。
- 移动和回滚拒绝覆盖已有文件。
- TODO/日程候选确认/拒绝。
- 便利贴持久化和自动保存到收集箱 Markdown。

后续测试重点：

- 使用真实 MiMo key 和真实音频/图片/PDF 做网络联调，并记录失败状态和用量账本。
- 对全局快捷键、窗口预热、隐藏窗口回收做 Windows 实机交互测试。
- 对文件监听做长时间运行、批量写入、文件占用和冲突恢复测试。

## 协作规则与要点

- 每次开发前先阅读当前代码与文档。
- 不在需求不明确时直接实现。
- 将不确定的决策点交给用户确认。
- README 只维护当前项目事实，不作为流水账式变更日志。
- 当架构、目标、需求或当前状态变化时，同步更新 README。
