# TheBrain 架构

## 1. 总体形态

TheBrain 是 Tauri 桌面应用，不是传统远程 Web 前后端分离架构。

```text
React + Vite UI
  -> src/api.ts
  -> Tauri commands
  -> Rust services
  -> Vault filesystem + .thebrain/index.sqlite
```

前端负责用户界面、交互状态和 Tauri command 调用。本地 Rust 服务负责文件系统、SQLite、路径安全、AI 调用、索引、移动、回滚和日志。

## 2. 数据边界

```text
Vault/
  用户 Markdown、图片、音频、文本等文件

Vault/000-收集箱/
  低摩擦输入区，AI 自动整理的主要入口

Vault/.thebrain/
  内部索引、队列、日志、Trace、RAG、预算、移动记录

Vault/.secrets/
  可选本地凭据，gitignored
```

原则：

- Vault 文件是用户资产。
- `.thebrain` 是内部状态，不是用户笔记。
- `.secrets` 不进入 Git，不进入 RAG，不进入日志。
- 用户应能在不使用 TheBrain 的情况下继续读取 Vault 中的 Markdown 文件。
- 产品数据默认 Markdown/YAML-first：用户可读、可迁移、可长期保留的事实优先写入 Markdown、YAML、目录结构和 wikilink。
- SQLite 只承担 Markdown 不适合承载的内部能力：索引、embedding、队列、预算、移动日志、Trace、审计、缓存、命中统计和查询加速。
- 如果同一事实需要 Markdown 与 SQLite 双写，必须定义 Markdown 或 SQLite 哪一侧是 source of truth，并提供重建或修复路径。

## 3. 收集箱自动整理 Pipeline

目标 pipeline：

```text
新文件进入 000-收集箱
  -> 稳定等待与去重
  -> 内容抽取
  -> 读取 Archive Map（Vault 目录结构归档模板）
  -> AI 分类与目标路径决策
  -> 冲突检查与用户问答
  -> 自动移动
  -> 更新 ledger
  -> 写 movement log / audit events
  -> 生成 TODO / 日程 / 档案候选
  -> 可回滚
```

Archive Map / 动态归档模板是目标 pipeline 中的核心上下文服务，当前第一版已经接入。它与收集箱扫描不同：
- 收集箱扫描负责发现 `000-收集箱/` 中等待处理的文件。
- Archive Map 扫描负责读取正式 Vault 的目录结构，生成可归档目标地图，供 MiMo 做目标路径决策。

当前 Archive Map 第一版默认排除 `.thebrain/`、`.secrets/`、`.git/`、`node_modules/`、`target/`、`dist/` 等内部或工具目录，并完全排除 `000-收集箱/` 及其子目录。`000-收集箱/` 只作为待处理来源，不作为正式归档目标，也不提供归档分类结构。历史整理参考来自 `收集箱-已整理.md` 和 movement log 中的已完成移动记录。Archive Map 保存为用户可读 Markdown：`.thebrain/rules/archive-map.md`；SQLite 使用 `archive_map_runs` 和 `archive_map_entries` 缓存目录索引、生成时间、历史引用、本地启发式目录摘要、标题线索和内容线索。目录摘要只读取正式目录内少量 md/txt 文件前缀，不读取收集箱、内部目录、隐藏/工具目录或二进制/多格式内容。用户目录说明、整理提示和锁定状态保存到 `archive_map_directory_rules`，并镜像为 `.thebrain/rules/archive-map-rules.md`，只允许绑定当前正式 Archive Map 目录。读取 Archive Map snapshot 时会只读扫描当前正式目录，与上次缓存结果比较，返回 `ok/stale/empty/fallback` 健康状态、失效目录列表、目录摘要、目录规则，并从 `movement_log.status = 'moved'` 聚合历史归档命中排行；这些健康字段和规则字段不触发自动移动，也不会自动重建或修改 Vault 目录。

AI 整理决策必须优先选择 Archive Map 中已有目录。只有当已有目录确实不适合时，才允许建议新建目录；新建目录、低置信度、目标冲突或分类边界不清必须进入 conflict/用户确认流程。当前手动“生成计划”和后台 worker 已经通过同一个整理决策入口共用 Archive Map。

安全边界：

- 只处理 `000-收集箱/`。
- 不移动 `收集箱-已整理.md`。
- 不越过 Vault。
- 不覆盖已有文件。
- 所有移动有日志。
- 所有移动可回滚。

冲突处理：

- 命名冲突、目标路径冲突、分类概念冲突和低置信度整理结果不能静默处理。
- 当前第一版进入可见 conflict 状态，前端在收集箱侧栏展示冲突来源、目标、原因、只读源/目标预览、重命名建议、用户答案输入和相似规则推荐。
- 用户回答会沉淀为本地规则。规则正文保存到可读 Markdown：`.thebrain/rules/inbox-organizing-rules.md`；`.thebrain/index.sqlite` 保存 `conflict_rules`、`conflict_rule_hits`、审计事件和应用状态索引。规则启用/禁用和字段编辑当前只更新 SQLite 与 audit，Markdown 规则文件保持 append-only。
- 规则文件属于系统辅助文件，不能被收集箱 watcher 当作普通待整理文件反复处理，也不能被 RAG 当作用户内容索引。
- 第一版只做只读预览、推荐和用户确认应用；不静默覆盖、删除或批量自动应用规则。

后台 worker 与 MiMo 的关系：

- MiMo 是 AI provider，负责文本抽取、图片/音频理解、整理建议和问答生成。
- listener 是收集箱入口层，递归监听和扫描当前 Vault 的 `000-收集箱/`，负责跳过 ledger、内部目录、隐藏/临时文件、目录项和不支持的文件类型，并在文件稳定后用路径、mtime、size 签名去重入队。
- 后台 worker 是本地调度器，负责消费队列、稳定等待、调用 MiMo、检查预算、执行移动、写日志、处理重试和回滚。
- 当前 `worker.rs` 已实现第一层手动 drain worker：一次只 claim 一个 pending item，跳过 ledger、内部目录和非收集箱路径，支持 listener 和导入产生的队列项；只有抽取与整理决策均为可信 `ok` 且非 mock 时才移动文件；缺 key、预算阻断、解析失败、低置信度和目标冲突都会停在 failed/conflict 状态，并尝试附带相似冲突规则推荐。
- 当前 `commands.rs` 提供应用内 resident worker 第一版：用户可手动启动/停止一个 5 秒 tick 的常驻循环；它复用 `WorkerService::drain`，单并发、每 tick 最多处理 1 条队列项，不是系统服务，也不会随应用启动自动运行。
- 当前收集箱状态面板第一版汇总 listener、queue、worker、resident worker、conflict、Archive Map、movement log 和 audit timeline 状态，展示最近事件、最近错误；主操作区提供导入、递归扫描入队、重建归档地图、保存 Archive Map 目录规则、生成计划、运行整理、恢复并运行 worker、启动/停止常驻 worker 和启动/停止监听，状态面板保留 Archive Map 健康/失效/历史归档命中摘要、本地目录语义摘要、目录说明/锁定规则、失败/冲突队列项单项恢复、批量恢复只读预览/确认、冲突详情预览、动作选择、规则编辑/禁用和 movement log 单项回滚/批量回滚只读预览。
- 批量恢复命令只循环调用现有单项安全服务：队列批量重试/跳过复用 `QueueService::retry_item` / `skip_item`，movement log 批量回滚复用 `MovementService::rollback`，因此仍遵守状态校验、Vault 路径限制和不覆盖已有文件规则。`preview_queue_recovery` 和 `preview_rollback_moves` 是只读命令，只报告 eligible、blocking reason 和 warning，不写 audit、不写 ledger、不移动文件；真正执行时仍由原批量命令重新校验并写 audit。
- audit timeline 当前支持 `list_audit_events` 读取最近事件，以及 `search_audit_events` 按事件类型、文本、路径、queue id、movement id、状态、时间和 before id 做第一版搜索。前端提供类型筛选和文本搜索，但完整分页详情、日期范围 UI 和长期审计分析仍是后续目标。
- 只有 MiMo provider、listener、Archive Map、冲突规则服务、resident worker、批量恢复预览或状态面板不等于已经有完整后台自动整理；长期可信自动整理仍需要长时间实机监听验证、系统/托盘生命周期策略、更完整批量策略、真正 diff 视图和强确认移动动作。

## 4. Markdown 与元数据

Markdown frontmatter 只保存适合用户可见的小型元数据：

- `title`
- `tags`
- `aliases`
- `created`
- `updated`
- `source`
- `source_type`
- `status`
- `classification`
- `thebrain_id`

不写入 frontmatter 的内部数据：

- embedding
- AI prompt/response
- token usage
- 移动历史
- 队列日志
- Trace
- 访问频率明细

这些数据进入 `.thebrain/index.sqlite`。

## 5. RAG Pipeline

当前 pipeline：

```text
扫描 Vault md/txt
  -> 跳过内部目录和 ledger
  -> Markdown 结构感知分块
  -> 写 rag_documents / rag_chunks
  -> 关键词检索
  -> local semantic placeholder
  -> 去重、归一化、top-k
  -> 构造上下文
  -> MiMo 或 fallback 回答
  -> 写引用和 Trace
```

后续目标：

- 真实 embedding。
- 向量检索。
- rerank。
- 多轮会话 UI 第一版已接入：会话列表、会话创建、会话详情读取、user/assistant 消息持久化，以及全库、当前文件、当前目录/项目前缀范围检索。
- 后续仍需会话重命名、删除、历史搜索、引用筛选和跨会话长期记忆策略。
- 跨文件长期记忆策略。
- 更强引用定位和打开能力。

## 6. TODO、日程和档案

TheBrain 正在建立从内容到行动和档案的模型：

```text
会议纪要 / 通话记录 / 笔记 / 便利贴
  -> 抽取人物、地点、联系方式、任务、时间
  -> 生成候选
  -> 自动或半自动写入 TODO / 日程 / 人名档案
  -> 保留来源引用
```

当前第一增量已经完成 TODO/日程候选到正式行动项的内部闭环：

- `action_candidates` 保存 TODO/日程候选，来源可以是收集箱整理计划、便利贴或后续抽取服务。
- `todo_items` 保存正式 TODO，包含来源候选、来源文件、标题、备注、截止时间、原始 payload、状态和完成/取消时间。
- `schedule_items` 保存正式日程，包含来源候选、来源文件、标题、备注、开始/结束时间、全天、时区、地点、原始 payload、状态和完成/取消时间。
- `promote_todo_schedule_candidate` 对同一候选幂等创建正式项，并把候选标记为 confirmed；被 rejected 的候选不能 promotion。
- `list_todo_items`、`list_schedule_items`、`set_todo_item_status`、`set_schedule_item_status` 提供第一层列表、完成和取消能力。
- 正式行动项当前是 `.thebrain/index.sqlite` 内部状态；不会写入 Markdown frontmatter，也不代表已经有提醒通知、重复日程、跨设备同步或完整 TickTick 式任务系统。

关键决策尚未完成：

- 档案数据写入 Markdown、SQLite，还是两者结合。
- TODO/日程是否允许自动确认、何时需要批量确认或人工复核。
- TODO/日程是否需要 Markdown/YAML 双写，以及双写后的 source of truth 和重建策略。
- 联系方式等敏感信息如何显示、导出和保护。

## 7. 前端与后端关系

前端可以先展示完整产品愿景和交互壳，但必须清楚标注状态：

- 已可用。
- 实验。
- 占位。
- 后续。
- fallback。

后端逐步补齐真实能力。前端不应把未实现后端能力伪装成真实成功。

个人页和项目工作台已接入 Workspace Insights 第一版：后端通过 `get_workspace_insights` 只读扫描正式 Vault 文件、项目目录和最近文件，并从 SQLite 聚合 RAG、候选、正式行动项、movement、audit 和 sticky 计数；前端优先使用这些真实统计。学习曲线、项目级 Agent 历史、联系人档案、提醒通知和复杂长期统计仍按后续能力处理，不能描述为已完成。

## 8. 当前主要服务

- `vault.rs`：Vault 初始化和路径安全。
- `importer.rs`：导入到收集箱。
- `listener.rs`：收集箱递归监听/扫描、稳定等待、跳过规则、去重入队和 listener 状态。
- `ai.rs`：MiMo、抽取、整理决策和 fallback。
- `worker.rs`：收集箱队列第一层消费、暂停恢复、可信移动、失败/冲突审计；手动 worker 与应用内 resident worker 都复用这一服务。
- `movement.rs`：文件移动、空目录清理、ledger、audit、回滚和只读回滚预览。
- `archive_map.rs`：扫描正式 Vault 目录结构，生成 `.thebrain/rules/archive-map.md` 和 SQLite 缓存，供整理计划和 worker 共享；从正式目录少量 md/txt 文件生成本地启发式目录摘要、标题线索和内容线索；目录说明/整理提示/锁定状态保存到 `archive_map_directory_rules` 并镜像为 `.thebrain/rules/archive-map-rules.md`；读取 snapshot 时只读返回健康状态、失效目录、目录摘要、目录规则和 movement log 历史归档命中统计。
- `audit.rs`：审计事件写入、最近事件列表、按类型查询和第一版 search。
- `conflict_rules.rs`：冲突详情、bounded preview、只读重命名建议、用户答案规则 Markdown、规则索引、启用/禁用/编辑、命中记录、相似规则推荐和确认应用。
- `queue.rs`：队列状态、claim/retry、失败/冲突单项恢复、只读恢复预览所需读取和 listener 状态持久化。
- `rag.rs`：RAG 索引、会话消息和问答编排。
- `retrieval.rs`：检索通道、范围过滤和评分。
- `chunking.rs`：Markdown 分块。
- `rag_trace.rs`：Trace 持久化。
- `action_items.rs`：TODO/日程候选 promotion、正式 TODO/日程列表和状态更新。
- `workspace_insights.rs`：个人页和项目工作台的只读统计聚合。
- `index.rs`：SQLite schema。
- `budget.rs` / `usage.rs`：预算和用量。
- `sticky.rs`：便利贴。
- `candidates.rs`：TODO/日程候选。
