# TheBrain

TheBrain 是一个本地优先的个人外置大脑桌面应用。它以类 Obsidian 的 Markdown Vault 为基础，把收集箱、AI 自动整理、文件问答、TODO、档案和长期记忆连接成一个可追溯、可回滚的个人知识工作台。

当前产品优先服务学生和个人知识工作者，第一目标用户是项目创建者本人；未来会逐步开源给更广泛的个人知识管理用户使用。

## 当前状态

项目当前处于 v0.8 TODO/日程第一增量阶段，同时继续打磨 v0.6 收集箱自动整理和 v0.7 RAG 能力。Tauri 2 + React + Vite + Rust 桌面骨架已经可运行，当前实现包含：

- 本地 Vault 选择与初始化。
- `000-收集箱/` 与 `000-收集箱/收集箱-已整理.md`。
- `.thebrain/index.sqlite` 内部索引、日志和状态数据库。
- Markdown 读写、预览、导出和 YAML frontmatter 过滤。
- 收集箱导入、抽取、整理计划、移动、ledger、movement log、audit events 和回滚。
- 收集箱递归展示、监听与自动入队第一版：递归处理当前 Vault 的 `000-收集箱/` 子目录，跳过 ledger、内部目录、隐藏/临时文件、目录项和不支持的文件类型，稳定等待后基于路径、mtime、size 去重入队。
- Archive Map / 动态归档模板第一版：扫描正式 Vault 目录结构，完全排除 `000-收集箱/`、`.thebrain/`、`.secrets/` 和工具目录，生成 `.thebrain/rules/archive-map.md`，并把目录地图接入手动“生成计划”和 worker 共用的整理决策 prompt；当前还会在读取地图时做只读健康检查，展示缓存目录是否相对当前正式 Vault 目录失效、基于 movement log 的历史归档命中排行、本地启发式目录语义摘要、用户目录说明/锁定规则第一增量，以及整理决策的 Archive Map 命中、新目录建议和确认原因元数据。
- 收集箱队列的第一层可信 worker：手动单轮消费 pending 队列、暂停/恢复、稳定等待、预算检查、MiMo 抽取与整理决策、非 mock 且可信时自动移动、失败/冲突写入内部状态；低置信度、目标冲突、收集箱内目标或新目录建议会写入结构化确认原因，供队列、audit 和冲突 UI 展示。
- 应用内 resident worker 第一版：用户可在收集箱页手动启动/停止 5 秒 tick 的常驻消费循环；它复用 `WorkerService::drain`，单并发、每 tick 最多处理 1 条队列项，不是系统服务，也不会随应用启动自动运行。
- 冲突问题与规则记忆第一版：前端可查看 open conflict、只读预览源/目标文件状态、文本片段和 bounded 只读 diff、选择处理动作、使用只读重命名建议、记录用户答案、写入 `.thebrain/rules/inbox-organizing-rules.md`，并用 `.thebrain/index.sqlite` 保存规则索引、命中、启用/禁用/编辑状态和审计；相似冲突会优先展示推荐规则，用户确认后才应用，不做静默覆盖或删除。
- 收集箱整理状态面板与恢复动作第一版：集中展示 listener、queue、worker、resident worker、conflict、movement log 和 audit timeline 的状态、最近事件和最近错误；主操作区提供导入、递归扫描入队、生成计划、运行整理、恢复并运行 worker、启动/停止常驻 worker 和启动/停止监听，状态面板保留失败/冲突队列项单项恢复、批量恢复预览/确认、整理计划确认原因、audit 日期范围筛选/分页加载/详情展开、冲突规则处理、movement log 单项回滚和批量回滚预览/确认。
- MiMo provider 路径、设置页 key 保存、BOM 污染检测、预算状态、用量占位账本和 fallback/pending 状态。
- md/txt 本地 RAG 索引、关键词检索、local semantic placeholder、引用、引用筛选、Trace、会话历史、会话重命名/删除/搜索和范围检索第一版；RAG/MiMo 问答通过后台任务运行，避免长时间 AI 响应阻塞桌面 UI。
- 前端工作台：主仪表盘、收集箱、Markdown 编辑器、便利贴、个人页、项目页和设置页；其中个人页和项目页已接入 Workspace Insights 第一版真实统计和正式 TODO/日程第一增量，学习曲线、项目级 Agent 历史、提醒通知和联系人档案仍是占位或后续目标。
- TODO/日程正式行动系统第一增量：`action_candidates` 可 promotion 为 SQLite 内部的 `todo_items` / `schedule_items`，支持幂等创建、列表展示、完成和取消状态更新；当前不提供提醒通知、重复日程、Markdown 双写、人名档案或联系人关系。

当前阶段不是完整个人 Agent、真实向量数据库系统或完整多格式解析系统。当前 RAG 使用关键词通道和 local semantic placeholder 通道，不声明已经具备真实 embedding 或重排能力。PDF、DOCX、PPTX、URL 的真实解析仍是后续目标。

当前能力状态约定：

- 真实可用：已有前端入口、Tauri command、本地服务、Vault 或 SQLite 数据支撑。
- fallback：后端命令失败或运行环境不完整时的安全降级结果，必须显示原因，不代表能力已完成。
- 占位：按设计图或产品愿景保留的 UI 与样例数据，暂不接正式后端。
- 后续目标：已进入 PRD/Roadmap，但当前阶段不要求实现。

占位能力真实接入后，应删除对应占位数据、占位注释和 README 中的占位描述。

## 产品定位

TheBrain 的长期方向是：

- Obsidian 增强版：以本地 Markdown Vault 作为用户数据源。
- NotebookLM 增强版：对整理后的本地文件进行带引用的问答、总结、对比和行动建议。
- 滴答清单补充：从笔记、会议纪要、通话记录和收集箱内容中整理 TODO、日程和提醒。
- 个人第二大脑：管理学习、生活、项目、人际关系和长期档案。

第一核心场景是类 Obsidian 的收集箱与 AI 自动分类整理。快速便利贴用于低摩擦捕获灵感；文件问答、TODO、个人档案和项目工作台围绕整理后的 Vault 逐步展开。

## AI 自动整理边界

AI 可以在用户指定的 Vault 范围内自动整理并移动 `000-收集箱/` 中的文件，不要求每次移动都由用户手动确认。

自动整理必须遵守以下边界：

- 只能处理 `000-收集箱/` 下允许处理的文件。
- 不能移动 `000-收集箱/收集箱-已整理.md`。
- 不能越过 Vault 根目录，所有路径必须规范化并拒绝绝对路径、空路径和 `..` 逃逸。
- 不能覆盖已有文件；冲突必须进入可见 conflict 状态或采用明确的冲突策略。
- 每次移动都必须写入 ledger、movement log 和 audit events。
- 必须支持回滚，把文件移回收集箱并更新 ledger。
- embedding、AI prompt/response、token usage、移动历史、队列日志不写入 Markdown frontmatter，只进入 `.thebrain` 内部结构。
- TODO、日程、联系人档案等行动项可以由 AI 生成候选；是否自动写入正式系统需要在后续 PRD 中按场景定义。

当前代码已经具备收集箱递归展示、递归扫描入队、监听入队、移动、空目录清理、日志、回滚、手动触发的单并发队列消费能力、手动启动/停止的应用内 resident worker 第一版、Archive Map 第一版、Archive Map 健康/失效/历史归档命中展示第一增量、Archive Map 本地目录语义摘要第一增量、Archive Map 用户目录说明/锁定规则第一增量、整理决策确认元数据第一增量、冲突问答与规则记忆第一版，以及收集箱状态面板、audit timeline 搜索/类型筛选/日期范围/分页/详情展开第一版、队列批量恢复预览/确认和 movement log 批量回滚预览/确认第一版；整理计划、手动 worker 和 resident worker 已共用 Archive Map 上下文，低置信度、目标冲突、收集箱内目标或不在 Archive Map 中的新目录建议会停在 pending/conflict 状态，并在计划详情、队列 payload、audit payload 和冲突详情中保留结构化确认原因；冲突详情当前已有 bounded 只读 diff 预览第一增量；系统级后台服务、应用启动自启、长时间实机监听稳定性、更完整批量处理策略、更完整的 merge/diff 体验和更完整的自动冲突处理仍是后续目标。

Archive Map 的设计边界是：只扫描正式 Vault 归档目录，完全排除 `000-收集箱/` 及其子目录。`000-收集箱/` 只作为待处理来源，不作为归档目标，也不提供归档分类结构；历史参考只能来自 `收集箱-已整理.md`、movement log 或 audit events 中的已完成移动记录。

## 数据边界

Vault 是用户资产的唯一可信来源；`.thebrain` 是索引、日志、Trace、队列和状态存储，不应污染用户 Markdown。

TheBrain 默认采用 Markdown/YAML-first：用户可读、可迁移、可长期保留的事实优先写入 Markdown、YAML frontmatter、目录结构、wikilink 和可读 ledger；SQLite 只承担索引、embedding、队列、预算、移动日志、审计、Trace、缓存和查询加速等 Markdown 不适合承载的内部状态。

```text
Vault/
  000-收集箱/
    收集箱-已整理.md
  .thebrain/
    index.sqlite
    rules/
      archive-map.md
      archive-map-rules.md
      inbox-organizing-rules.md
  .secrets/
    mimo_api_key.txt   # 可选，gitignored，不提交
```

`.thebrain/index.sqlite` 当前包含：`vault_meta`、`file_index`、`ai_usage`、`audit_events`、`listener_state`、`queue_items`、`dedupe_records`、`budget_settings`、`budget_ledger`、`movement_log`、`conflict_rules`、`conflict_rule_hits`、`archive_map_runs`、`archive_map_entries`、`archive_map_directory_rules`、`action_candidates`、`todo_items`、`schedule_items`、`sticky_notes`、`rag_documents`、`rag_chunks`、`rag_index_runs`、`rag_queries`、`rag_trace_runs`、`rag_trace_nodes`、`rag_conversations`、`rag_messages`。

## 凭据与安全

真实 API Key 只允许存在于以下位置：

- 本机环境变量 `MIMO_API_KEY`
- gitignored 的 `Vault/.secrets/mimo_api_key.txt`
- 开发环境当前工作目录下 gitignored 的 `.secrets/mimo_api_key.txt`
- 用户显式传入的运行时参数

真实 key 不进入 README、代码、测试、日志、SQLite seed、前端错误信息或提交内容。仓库中的 `.env.local.example` 只能使用占位符。

## 支持文件类型

当前真实处理类型：

- 文本：`md`、`markdown`、`txt`
- 音频：`mp3`、`wav`、`m4a`、`aac`
- 图片：`png`、`jpg`、`jpeg`

当前 RAG 索引只扫描 Vault 内的 `md`、`markdown`、`txt`，并跳过 `.thebrain/`、`.secrets/`、`.git/`、`node_modules/`、`target/`、`dist/` 和 `000-收集箱/收集箱-已整理.md`。

PDF、DOCX、PPTX、URL 导入和复杂多格式解析可以在 UI 中作为未来能力展示，但必须明确标注为实验、占位或后续能力，避免让用户误解为已经完整可用。

## 当前架构

TheBrain 不是传统远程前后端分离 Web 应用，而是 Tauri 桌面应用。它分为前端 UI 层和本地后端服务层，最终打包在同一个桌面应用里。

前端层：

- `src/App.tsx`：桌面工作台 shell，包含 Vault、收集箱、Markdown、AI 整理、RAG 问答、TODO/日程、便利贴、个人页、项目页和设置页。
- `src/api.ts`：封装 Tauri commands，暴露受控导入、收集箱 listener、Archive Map 健康字段和目录规则保存、MiMo 状态、抽取、整理计划与确认原因元数据、worker、resident worker、queue 单项/批量恢复、批量恢复预览、移动、单项/批量回滚、批量回滚预览、audit timeline 搜索、Workspace Insights、正式 TODO/日程、冲突详情/预览/只读 diff、冲突规则管理和 RAG 会话/重命名/删除/搜索/问答命令。
- `src/styles.css`：视觉 token、左侧导航、页面网格、收集箱、RAG 面板、Markdown 三栏编辑器和便利贴布局。

本地后端服务层：

- `src-tauri/src/commands.rs`：暴露 Vault、Markdown、收集箱、ledger、导入、Archive Map/目录规则、MiMo、listener、队列、worker、resident worker、预算、移动/回滚、audit timeline/search、批量恢复预览、候选、正式 TODO/日程、便利贴、快捷键、冲突预览/规则管理和 RAG commands。
- `src-tauri/src/services/vault.rs`：Vault 初始化和路径规范化。
- `src-tauri/src/services/importer.rs`：受控复制/移动外部文件到收集箱。
- `src-tauri/src/services/listener.rs`：收集箱递归扫描与监听入队规则、稳定等待、临时/隐藏/内部/不支持文件跳过、mtime/size 去重和 listener 状态更新。
- `src-tauri/src/services/archive_map.rs`：扫描正式 Vault 目录结构，生成 `.thebrain/rules/archive-map.md` 和 SQLite 缓存，供整理计划和 worker 共享目标目录上下文；读取 snapshot 时只读比较缓存目录与当前正式目录，并基于 movement log 生成历史归档命中统计；从正式目录内少量 md/txt 文件的标题与短文本片段生成本地启发式目录摘要；目录说明/整理提示/锁定规则保存到 `archive_map_directory_rules`，并镜像到 `.thebrain/rules/archive-map-rules.md`。
- `src-tauri/src/services/ai.rs`：MiMo 状态、文本本地抽取、音频/图片 MiMo 抽取、结构化整理 JSON 解析、Archive Map 命中/新目录/确认原因元数据派生和 fallback。
- `src-tauri/src/services/worker.rs`：收集箱队列的第一层可信消费器，负责 claim pending item、稳定等待、预算检查、调用 MiMo、可信移动、失败/冲突/audit 记录和 drain；手动 worker 与 resident worker 都复用这一服务。
- `src-tauri/src/services/movement.rs`：收集箱文件移动、移动后空目录清理、ledger、movement log、audit events 和回滚。
- `src-tauri/src/services/conflict_rules.rs`：冲突问题详情、源/目标文件 bounded preview、bounded 只读 diff 预览、只读重命名建议、用户答案规则写入、规则索引、启用/禁用/编辑、相似规则推荐和确认后应用。
- `src-tauri/src/services/action_items.rs`：正式 TODO/日程第一增量，负责把候选幂等 promotion 为 `todo_items` / `schedule_items`，并维护完成/取消状态。
- `src-tauri/src/services/workspace_insights.rs`：个人页和项目工作台的只读统计聚合，扫描正式 Vault 文件、项目目录和最近文件，并从 SQLite 聚合 RAG、候选、正式行动项、movement、audit 和 sticky 计数。
- `src-tauri/src/services/rag.rs`、`retrieval.rs`、`chunking.rs`、`rag_trace.rs`：本地 RAG 索引、范围检索、会话消息、会话重命名/删除/搜索、分块、引用和 Trace。
- `src-tauri/src/services/index.rs`：`.thebrain/index.sqlite` schema。

MiMo 是 AI provider，负责抽取、理解、整理建议和回答生成；listener 只负责发现 `000-收集箱/` 中允许处理的普通文件并稳定去重入队，递归扫描和递归 watcher 会覆盖子目录，但会跳过 ledger、`.thebrain`、`.secrets`、隐藏/临时文件和不支持的文件类型；Archive Map 扫描正式 Vault 归档目录并生成 `.thebrain/rules/archive-map.md`，从正式目录内少量 md/txt 文件生成本地启发式目录摘要，目录规则保存到 SQLite 并镜像为 `.thebrain/rules/archive-map-rules.md`，整理计划和后台 worker 通过同一个整理决策入口读取这份地图、摘要与规则，并把 Archive Map 命中、新目录建议、是否需要确认和确认原因作为结构化元数据返回；后台 worker 是本地调度器，负责从队列取任务、稳定等待、预算检查、调用 MiMo、执行移动、写日志、重试和回滚。当前已接入第一层手动 drain worker 和手动启动/停止的应用内 resident worker：它们只处理 `000-收集箱/` 中的普通文件；只有抽取和整理决策均为 `ok` 且 `is_mock=false` 时才会移动文件，移动成功后会清理收集箱内变空的父目录。冲突规则第一版只做只读预览、bounded 只读 diff、推荐与用户确认应用，规则 Markdown 位于 `.thebrain/rules/`，不会被 listener 或 RAG 当作用户内容处理；规则编辑/禁用只更新 SQLite 和 audit，不重写历史 Markdown 规则条目。收集箱状态面板第一版会把 listener、queue、worker、resident worker、conflict、Archive Map、movement log 和 audit timeline 汇总到同一页面，并提供队列单项/批量重试、跳过以及 movement log 单项/批量回滚。resident worker 当前不是系统服务，不随应用启动自启；长时间实机稳定性、更完整批量策略和完整冲突恢复体验仍是后续目标。

## 使用方式

1. 运行 `npm install` 安装前端依赖。
2. 运行 `npm run tauri dev` 启动桌面应用，或运行 `npm run build` 做前端构建验证。
3. 在应用中选择 Vault 并初始化。
4. 可选：在设置页保存 MiMo API key，或设置 `MIMO_API_KEY`，或把 key 放入 `Vault/.secrets/mimo_api_key.txt` / 项目根目录 `.secrets/mimo_api_key.txt`。
5. 在收集箱中导入文件或用便利贴快速捕获内容。
6. 让 AI 对收集箱文件进行抽取、分类、移动、记录和回滚。
7. 重建 RAG 索引后，选择全库、当前文件或当前目录/项目前缀范围提问，并查看、筛选引用，搜索、重命名或删除会话历史，以及检查 Trace。

## 已完成目标

- 初始化并维护可运行的 Tauri 2 + React + Vite 应用骨架。
- Vault 选择、结构幂等初始化和 `.thebrain/index.sqlite` 创建。
- Markdown 编辑、预览、保存、导出和 YAML frontmatter 过滤。
- `000-收集箱` 递归文件展示与 ledger 相对 wikilink 历史映射。
- 受控导入命令 `import_to_inbox`，支持 copy/move、冲突不覆盖、audit/queue 记录。
- 收集箱 listener 命令 `get_inbox_listener_status`、`start_inbox_watcher`、`stop_inbox_watcher`、`scan_inbox_queue`，支持启动、停止、查询状态和递归手动扫描入队。
- Archive Map 命令 `get_archive_map`、`rebuild_archive_map`、`save_archive_map_directory_rule`，生成可读 `.thebrain/rules/archive-map.md`，缓存正式目录、样本文件、关键词线索、本地启发式目录摘要和历史归档引用；`get_archive_map` 返回只读健康状态、失效目录列表、基于 movement log 的历史归档命中排行和目录规则；目录规则保存到 SQLite 并镜像为 `.thebrain/rules/archive-map-rules.md`，只允许绑定当前正式归档目录。
- MiMo 状态命令 `get_mimo_status`，不泄露 key。
- 抽取命令 `extract_with_mimo`，支持文本本地抽取和音频/图片 MiMo 调用路径。
- 单文件计划命令 `plan_inbox_item`，返回 extraction、structured plan、候选、预算状态，以及 Archive Map 命中、新目录建议和确认原因元数据。
- 第一层 worker 命令 `get_worker_status`、`run_inbox_worker`、`pause_inbox_worker`、`resume_inbox_worker`，支持手动消费队列、暂停恢复、失败/冲突状态展示，并把整理决策确认元数据写入 worker 结果、queue payload 和 audit payload。
- resident worker 命令 `get_resident_worker_status`、`start_resident_worker`、`stop_resident_worker`，支持用户手动启动/停止应用内常驻消费循环，默认 5 秒 tick、单并发、每 tick 最多处理 1 条队列项。
- 收集箱整理状态面板，集中展示 listener、queue、worker、resident worker、conflict、movement log、audit timeline、最近事件和最近错误，并支持 audit 搜索、类型筛选、日期范围、分页加载和详情展开第一版。
- 队列单项/批量恢复命令 `retry_queue_item`、`skip_queue_item`、`retry_queue_items`、`skip_queue_items` 和只读预览命令 `preview_queue_recovery`，支持失败/冲突/运行中项目恢复为 pending，或把待处理/失败/冲突/运行中项目标记为 skipped；前端批量操作会先展示 eligible/blocked 预览，再由用户确认执行。
- 自动/手动移动基础命令，更新 ledger、movement log、audit events。
- movement log 列表命令 `list_move_logs`、单项回滚命令 `rollback_move`、批量回滚命令 `rollback_moves` 和只读预览命令 `preview_rollback_moves`，不覆盖已有文件；前端批量回滚会先展示当前文件/恢复目标/阻塞原因，再由用户确认执行。
- audit timeline 命令 `list_audit_events` 和 `search_audit_events`，支持读取最近 audit events，按类型、文本、路径、状态、时间范围和 cursor 搜索；收集箱页面已接入类型/文本/日期范围筛选、分页加载和 bounded payload 详情展开。
- TODO/日程候选创建、确认、忽略。
- TODO/日程正式行动项第一增量：`promote_todo_schedule_candidate`、`list_todo_items`、`list_schedule_items`、`set_todo_item_status`、`set_schedule_item_status` 已接入；promotion 对同一候选幂等，不重复创建正式项。
- 预算暂停/耗尽状态和 `ai_usage` 用量占位记录。
- 冲突事件列表、详情、bounded preview、bounded 只读 diff 预览、只读重命名建议、解决记录、用户答案写入规则、相似规则推荐和确认应用命令。
- 冲突规则 Markdown 文件 `.thebrain/rules/inbox-organizing-rules.md` 与 SQLite 规则索引/命中记录；SQLite 规则支持启用/禁用和字段编辑，Markdown 规则文件保持 append-only。
- 设置页展示 MiMo key 状态，只显示 key 来源和脱敏状态；支持保存 key 到 Vault `.secrets/mimo_api_key.txt`，并检测/清理 UTF-8 BOM 污染。
- 全局快捷键插件依赖和注册命令已接入。
- RAG 文档、分块、索引运行、查询、Trace、会话和消息基础表。
- RAG 索引、状态、后台问答、引用、Trace、会话列表、会话详情、会话创建和范围检索基础命令。
- RAG 多轮会话 UI 与历史列表第一版，支持新建/打开会话、保存 user/assistant 消息、会话重命名、会话历史删除、按标题/消息搜索历史、按引用路径/标题/片段和检索 channel 筛选当前回答引用，并在提问时选择全库、当前文件或当前目录/项目前缀范围。
- Workspace Insights 命令 `get_workspace_insights` 和前端接入第一版：个人页/项目页可读取 Vault 文件、Markdown、项目目录、最近文件、RAG、候选、正式行动项、movement、audit 和 sticky 真实统计。

## 未完成目标

- Archive Map 增强：第一版已能扫描正式 Vault 目录、生成可读地图并接入整理决策；当前已接入只读健康检查、失效目录提示、历史归档命中排行、本地启发式目录语义摘要、用户目录说明/锁定规则第一增量，以及新目录/低置信度/冲突确认元数据第一增量。后续仍需更强的 AI/长期语义总结、规则生命周期与批量编辑、长期趋势可视化和更完整的新目录建议确认交互。
- resident worker 增强：当前第一版只是用户手动启动/停止的应用内循环；后续仍需系统级后台服务或托盘生命周期策略、应用启动自启策略、长时间实机监听稳定性、批量处理策略和更完整的自动整理策略。
- 真实 MiMo API 的长时间、多文件类型、带成本实机联调。
- MiMo 返回 token/cost 的真实精确统计。
- 完整冲突解决 UI 增强：当前已有只读重命名建议、bounded preview、bounded 只读 diff 预览、动作选择、规则启用/禁用/编辑第一版；后续仍需更完整的 side-by-side/merge diff 体验、批量冲突处理、强确认的覆盖/重命名执行流和更细的恢复体验。
- 更完整的状态面板和恢复体验：当前已有 audit 搜索/类型筛选/日期范围/分页/详情展开、队列批量恢复预览/确认和 movement 批量回滚预览/确认第一版；后续仍需长期审计分析、批量恢复差异预览、冲突恢复策略和更细粒度确认。
- 真实 embedding、向量检索、重排模型、跨会话长期记忆策略和复杂个人统计。
- RAG 会话增强：当前已有会话重命名、删除、按标题/消息搜索历史和当前回答引用筛选第一增量；后续仍需引用排序/固定、删除恢复/归档策略和更完整的跨会话长期记忆策略。
- PDF、DOCX、PPTX、URL 的真实解析。
- 人名档案、联系人信息、会议纪要、通话记录和个人关系管理数据模型。
- TODO/日程完整工作流增强：当前只有 SQLite 内部正式项第一增量；后续仍需提醒通知、重复日程、优先级、项目/文件引用定位、Markdown/YAML 双写或重建策略、批量确认、搜索过滤和更完整编辑体验。
- 便利贴独立多窗口池、窗口回收和更完整的托盘体验。
- 个人页和项目工作台增强：当前已有 Workspace Insights 第一版真实统计和正式行动项第一增量；后续仍需项目级 Agent 历史、学习曲线、联系人档案、项目级任务筛选增强和更复杂统计接入真实数据。
- `src/App.tsx` 页面与状态继续拆分，把个人页、项目页、RAG、收集箱、便利贴等不相关模块拆成更清晰的组件和服务边界。
- 端到端 GUI 自动化测试和真实 Windows 实机长时间验证。

## 文档入口

- [PRD](docs/PRD.md)：产品定位、用户、核心场景、范围和非目标。
- [交付物关系](docs/DELIVERABLES.md)：README、AGENTS、PRD、架构、路线图、调研文档的职责分工。
- [架构](docs/ARCHITECTURE.md)：Vault、`.thebrain`、AI 整理、RAG 和 Tauri 分层。
- [路线图](docs/ROADMAP.md)：v0.5 及后续阶段目标。
- [同类项目调研](docs/research/comparable-projects.md)：Obsidian、Logseq、AFFiNE、Anytype、Joplin、Khoj、AnythingLLM 等项目参考。

## Test Plan

当前验证门槛：

- `npm install`
- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`

当前 Rust 单元测试覆盖 Vault 初始化、路径安全、Markdown frontmatter、ledger、SQLite schema、Archive Map 扫描/排除规则/Markdown 生成/历史引用、本地目录语义摘要、健康失效检测、movement log 历史归档命中统计、目录规则持久化/快照合并/路径拒绝、整理决策确认元数据、收集箱递归展示、listener 递归扫描/跳过规则/稳定等待/去重、队列 claim/retry/单项跳过、预算、audit 列表/搜索/日期范围/cursor、导入、导入队列项 worker 处理、MiMo fallback、worker 暂停与失败不移动、worker 阻断元数据写入队列 payload、resident worker 保守参数、批量 ID 防误操作、移动/空目录清理/回滚、冲突规则 Markdown 写入、相似规则推荐、规则应用不覆盖、规则禁用排除匹配、只读重命名建议、冲突 bounded preview 和 bounded 只读 diff、规则路径不被 listener 处理、TODO/日程候选、正式 TODO/日程 promotion 和状态更新、便利贴、RAG 基础能力、RAG 会话持久化、RAG 会话重命名/删除/搜索、RAG 范围检索和 Workspace Insights 聚合；前端构建覆盖 RAG 引用筛选类型检查。
