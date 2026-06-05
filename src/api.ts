import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface CommandMeta {
  isFallback?: boolean;
  fallbackReason?: string;
}

export interface VaultInitResult {
  vaultPath: string;
  created: string[];
  preserved: string[];
  indexPath: string;
}

export interface VaultTreeNode {
  name: string;
  relativePath: string;
  isDir: boolean;
  children: VaultTreeNode[];
}

export interface InboxItem {
  name: string;
  relativePath: string;
  isDir: boolean;
  sizeBytes?: number;
  modifiedAt?: number;
}

export interface LedgerItem {
  lineNumber: number;
  sourceLine: string;
  rawTarget: string;
  targetRelativePath: string;
  displayName: string;
  exists: boolean;
}

export interface MarkdownDocument {
  relativePath: string;
  raw: string;
  content: string;
  previewMarkdown: string;
  frontmatter: unknown;
}

export interface MarkdownExport {
  relativePath: string;
  markdown: string;
}

export interface UsageSummary extends CommandMeta {
  isMock: boolean;
  provider: string;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  costCents: number;
  storage: string;
}

export interface RagIndexRun {
  id: number;
  status: string;
  startedAt: string;
  finishedAt?: string;
  scannedCount: number;
  indexedCount: number;
  skippedCount: number;
  deletedCount: number;
  chunkCount: number;
  error?: string;
}

export interface RagIndexStatus extends CommandMeta {
  schemaVersion: number;
  documentCount: number;
  chunkCount: number;
  lastRun?: RagIndexRun;
}

export type RagScope =
  | { kind: "allVault" }
  | { kind: "currentFile"; relativePath: string }
  | { kind: "projectPrefix"; relativePathPrefix: string };

export interface RagConversation extends CommandMeta {
  id: number;
  title: string;
  createdAt: string;
  updatedAt: string;
  lastMessageAt?: string;
  messageCount?: number;
}

export interface RagMessage extends CommandMeta {
  id: number;
  conversationId: number;
  role: "user" | "assistant" | "system" | string;
  content: string;
  createdAt: string;
  queryId?: number;
  status?: string;
  isMock?: boolean;
  fallbackReason?: string;
}

export interface RagConversationDetail extends CommandMeta {
  conversation: RagConversation;
  messages: RagMessage[];
}

export interface RagCitation {
  id: string;
  relativePath: string;
  title: string;
  headingPath: string[];
  snippet: string;
  score: number;
  channel: string;
  startLine: number;
  endLine: number;
}

export interface RagTraceNode {
  id: number;
  runId: number;
  parentId?: number;
  nodeType: string;
  name: string;
  input: unknown;
  output: unknown;
  status: string;
  startedAt: string;
  finishedAt?: string;
  durationMs?: number;
  error?: string;
}

export interface RagTraceRun {
  id: number;
  queryId?: number;
  status: string;
  startedAt: string;
  finishedAt?: string;
  durationMs?: number;
  metadata: unknown;
  nodes: RagTraceNode[];
}

export interface RagAnswer extends CommandMeta {
  queryId: number;
  conversationId?: number;
  provider: string;
  model: string;
  status: string;
  isMock: boolean;
  answer: string;
  fallbackReason?: string;
  retrievedCount: number;
  citations: RagCitation[];
  trace: RagTraceRun;
}

export interface MimoStatus extends CommandMeta {
  provider: string;
  extractModel: string;
  organizeModel: string;
  hasKey: boolean;
  keySource?: string;
  keyHasBom?: boolean;
  keyNeedsCleanup?: boolean;
  keyMessage?: string;
  status: "key_found" | "key_warning" | "missing_key" | string;
}

export interface ArchiveMapDirectory {
  relativePath: string;
  depth: number;
  fileCount: number;
  childCount: number;
  sampleFiles: string[];
  keywordHints: string[];
  historicalMoves: number;
}

export interface ArchiveMapRun {
  id: number;
  status: string;
  startedAt: string;
  finishedAt?: string;
  directoryCount: number;
  fileCount: number;
  historyCount: number;
  markdownPath: string;
  error?: string;
}

export interface ArchiveMapSnapshot extends CommandMeta {
  run: ArchiveMapRun;
  generatedAt: string;
  markdownPath: string;
  directoryCount: number;
  fileCount: number;
  historyCount: number;
  directories: ArchiveMapDirectory[];
  markdown: string;
}

export interface InboxImportResult extends CommandMeta {
  sourcePath: string;
  relativePath?: string;
  fileName?: string;
  mode: "copy" | "move" | string;
  status: "imported" | "already_inbox" | "conflict" | "unsupported" | "failed" | string;
  bytes?: number;
  auditId?: number;
  queueItemId?: number;
  error?: string;
}

export interface MimoExtractResult extends CommandMeta {
  provider: string;
  model: string;
  status: string;
  isMock: boolean;
  relativePath: string;
  text: string;
  error?: string;
}

export interface AppSettings extends CommandMeta {
  language: "zh-CN";
  organizationTemplate: string;
  aiDecisionModel: string;
  extractionModel: string;
  stickyNotesPath: string;
  autoSaveIntervalSeconds: number;
  queueConcurrency: number;
  retryLimit: number;
  cooldownMinutes: number;
  enableGlobalShortcut: boolean;
  prewarmWindows: number;
  activeWindowLimit: number;
  budgetMonthlyCents: number;
  budgetHardStopCents: number;
  conflictDefaultAction: ConflictResolutionAction;
}

export interface QueueStatus extends CommandMeta {
  state: "idle" | "running" | "paused" | "cooldown" | "error";
  active: number;
  pending: number;
  retrying: number;
  failed: number;
  conflicts?: number;
  completedToday: number;
  concurrency: number;
  retryLimit: number;
  cooldownUntil?: string;
  lastEvent?: string;
  updatedAt: string;
  items?: {
    pending: QueueItem[];
    running: QueueItem[];
    failed: QueueItem[];
    conflicts: QueueItem[];
    completed: QueueItem[];
  };
}

export interface QueueItem extends CommandMeta {
  id: number;
  kind: string;
  relativePath: string;
  status: string;
  dedupeKey?: string;
  payload: Record<string, unknown>;
  attempts: number;
  maxAttempts: number;
  lockedAt?: string;
  runAfter?: string;
  createdAt: string;
  updatedAt: string;
}

export interface WorkerStatus extends CommandMeta {
  listener: {
    enabled: boolean;
    status: string;
    watchPath?: string;
    lastEvent?: string;
    lastEnqueuedCount?: number;
    lastEventAt?: string;
    lastError?: string;
    updatedAt: string;
  };
  pending: number;
  running: number;
  failed: number;
  conflicts: number;
}

export interface ResidentWorkerStatus extends CommandMeta {
  running: boolean;
  vaultPath: string;
  intervalMs: number;
  maxItemsPerTick: number;
  stableWaitMs: number;
  startedAt?: string;
  lastTickAt?: string;
  lastStatus: string;
  lastProcessed: number;
  lastMoved: number;
  lastFailed: number;
  lastConflicts: number;
  lastError?: string;
  updatedAt: string;
}

export interface InboxListenerStatus extends CommandMeta {
  enabled: boolean;
  status: string;
  watchPath?: string;
  lastEvent?: string;
  lastEnqueuedCount: number;
  lastEventAt?: string;
  lastError?: string;
  updatedAt: string;
}

export interface WorkerRunItem {
  queueId: number;
  relativePath: string;
  status: string;
  message: string;
  movementId?: number;
}

export interface WorkerRunResult extends CommandMeta {
  status: string;
  processed: number;
  moved: number;
  skipped: number;
  failed: number;
  conflicts: number;
  items: WorkerRunItem[];
  listener: WorkerStatus["listener"];
}

export interface BudgetSettings {
  monthlyLimitCents: number;
  hardStopCents: number;
}

export interface BudgetStatus extends CommandMeta {
  provider: string;
  monthlyLimitCents: number;
  hardStopCents: number;
  spentCents: number;
  remainingCents: number;
  totalTokens: number;
  exhausted: boolean;
  updatedAt: string;
}

export interface OrganizeCandidate extends CommandMeta {
  id: string;
  sourceRelativePath: string;
  targetRelativePath: string;
  confidence: number;
  reason: string;
  tags: string[];
  conflict?: ConflictItem;
}

export interface AiOrganizePlan extends CommandMeta {
  id: string;
  summary: string;
  candidates: OrganizeCandidate[];
  createdAt: string;
}

export interface AiOrganizeResult extends CommandMeta {
  moved: number;
  skipped: number;
  conflicts: number;
  auditId?: string;
  message: string;
}

export interface InboxPlanResult extends CommandMeta {
  extraction: MimoExtractResult;
  plan: OrganizeDecisionResult;
  candidates: TodoScheduleCandidate[];
  budget: BudgetStatus;
}

export interface OrganizeDecisionResult extends CommandMeta {
  provider: string;
  model: string;
  status: string;
  isMock: boolean;
  sourceRelativePath: string;
  targetRelativePath: string;
  tags: string[];
  summary: string;
  reason: string;
  confidence: number;
  todoCandidates: unknown[];
  scheduleCandidates: unknown[];
  error?: string;
}

export interface MoveResult extends CommandMeta {
  sourceRelativePath: string;
  targetRelativePath: string;
  moved: boolean;
  auditId?: string;
  message: string;
}

export interface RollbackResult extends CommandMeta {
  auditId?: string;
  rolledBack: boolean;
  restoredRelativePath?: string;
  message: string;
}

export interface MoveLog extends CommandMeta {
  id: number;
  operation: string;
  sourceRelativePath: string;
  targetRelativePath: string;
  reason?: string;
  status: string;
  createdAt: string;
  movedAt?: string;
  rolledBackAt?: string;
  error?: string;
}

export interface AuditEvent extends CommandMeta {
  id: number;
  eventType: string;
  payload: Record<string, unknown>;
  createdAt: string;
}

export interface BatchOperationError {
  id: number;
  message: string;
}

export interface BatchQueueResult extends CommandMeta {
  requested: number;
  succeeded: QueueItem[];
  failed: BatchOperationError[];
}

export interface BatchRollbackResult extends CommandMeta {
  requested: number;
  rolledBack: MoveLog[];
  failed: BatchOperationError[];
}

export interface TodoScheduleCandidate extends CommandMeta {
  id: string;
  kind: "todo" | "schedule";
  sourceRelativePath: string;
  title: string;
  excerpt: string;
  dueAt?: string;
  confidence: number;
  status: "pending" | "confirmed" | "dismissed";
}

export interface StickyNote extends CommandMeta {
  id: string;
  title: string;
  content: string;
  autosavePath: string;
  pinned: boolean;
  updatedAt: string;
}

export type ConflictResolutionAction = "keep_existing" | "overwrite" | "rename" | "skip";

export interface ConflictPreviewFile {
  exists?: boolean;
  sizeBytes?: number;
  snippet?: string;
  relativePath?: string;
}

export interface ConflictPreview {
  sourceExists?: boolean;
  targetExists?: boolean;
  sourceSizeBytes?: number;
  targetSizeBytes?: number;
  sourceSnippet?: string;
  targetSnippet?: string;
  source?: ConflictPreviewFile;
  target?: ConflictPreviewFile;
  context?: string;
}

export interface ConflictRenameSuggestion {
  targetRelativePath: string;
  reason?: string;
  available?: boolean;
}

export interface ConflictItem extends CommandMeta {
  id: string;
  kind: "target_exists" | "source_missing" | "locked" | "out_of_vault" | "unknown";
  eventId?: number;
  createdAt?: string;
  status?: string;
  sourceRelativePath: string;
  targetRelativePath: string;
  message: string;
  options: ConflictResolutionAction[];
  recommendedAction: ConflictResolutionAction;
  recommendations?: ConflictRuleMatch[];
  preview?: ConflictPreview;
  context?: ConflictPreview;
  renameSuggestions?: ConflictRenameSuggestion[];
  payload?: Record<string, unknown>;
}

export interface ConflictRule {
  id: number;
  ruleKey: string;
  sourcePattern: string;
  targetPattern: string;
  answer: string;
  action: ConflictResolutionAction;
  autoApply: boolean;
  matchSummary: string;
  markdownPath: string;
  status: string;
  createdAt: string;
  updatedAt: string;
  hitCount: number;
}

export interface ConflictRuleUpdateInput {
  ruleId: number;
  status?: string;
  answer?: string;
  action?: ConflictResolutionAction;
  targetPattern?: string;
  autoApply?: boolean;
  matchSummary?: string;
}

export interface ConflictRuleUpdateResult extends CommandMeta {
  rule: ConflictRule;
  auditId: number;
  status: string;
}

export interface ConflictRuleMatch {
  rule: ConflictRule;
  score: number;
  reason: string;
  status?: string;
  autoApply?: boolean;
  hitCount?: number;
}

export interface ConflictAnswerInput {
  conflictId: string;
  answer: string;
  action?: ConflictResolutionAction;
  targetRelativePath?: string;
  autoApply?: boolean;
  matchSummary?: string;
}

export interface ConflictAnswerResult extends CommandMeta {
  conflictId: string;
  rule: ConflictRule;
  markdownPath: string;
  auditId: number;
  status: string;
}

export interface ApplyConflictRuleResult extends CommandMeta {
  conflictId: string;
  ruleId: number;
  status: string;
  moved: boolean;
  movementId?: number;
  message: string;
  auditId?: number;
}

export interface ConflictResolutionResult extends CommandMeta {
  conflictId: string;
  action: ConflictResolutionAction;
  resolved: boolean;
  message: string;
}

export const defaultAppSettings: AppSettings = {
  language: "zh-CN",
  organizationTemplate: "文档管理 + 知识标签",
  aiDecisionModel: "mimo-v2.5-pro",
  extractionModel: "mimo-v2.5",
  stickyNotesPath: "000-收集箱/便利贴草稿.md",
  autoSaveIntervalSeconds: 20,
  queueConcurrency: 1,
  retryLimit: 3,
  cooldownMinutes: 10,
  enableGlobalShortcut: false,
  prewarmWindows: 1,
  activeWindowLimit: 3,
  budgetMonthlyCents: 2000,
  budgetHardStopCents: 2500,
  conflictDefaultAction: "rename",
};

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return JSON.stringify(error);
}

function nowIso(): string {
  return new Date().toISOString();
}

async function invokeWithFallback<T>(
  command: string,
  args: Record<string, unknown>,
  fallback: (reason: string) => T,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    // Runtime fallback keeps the UI inspectable when a Tauri command is unavailable.
    // It must stay marked with fallbackReason and must not be treated as a completed backend capability.
    return fallback(toMessage(error));
  }
}

function fallbackUsage(reason: string): UsageSummary {
  return {
    isMock: true,
    provider: "mock",
    promptTokens: 0,
    completionTokens: 0,
    totalTokens: 0,
    costCents: 0,
    storage: ".thebrain/index.sqlite 未连接",
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackRagStatus(reason: string): RagIndexStatus {
  return {
    schemaVersion: 0,
    documentCount: 0,
    chunkCount: 0,
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackRagRun(reason: string): RagIndexRun & CommandMeta {
  return {
    id: 0,
    status: "failed",
    startedAt: nowIso(),
    finishedAt: nowIso(),
    scannedCount: 0,
    indexedCount: 0,
    skippedCount: 0,
    deletedCount: 0,
    chunkCount: 0,
    error: reason,
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackRagConversations(reason: string): (RagConversation & CommandMeta)[] {
  void reason;
  return [];
}

function fallbackRagConversation(title: string | undefined, reason: string): RagConversation {
  return {
    id: 0,
    title: title?.trim() || "未保存的 RAG 对话",
    createdAt: nowIso(),
    updatedAt: nowIso(),
    messageCount: 0,
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackRagConversationDetail(conversationId: number, reason: string): RagConversationDetail {
  return {
    conversation: {
      id: conversationId,
      title: "RAG 对话不可用",
      createdAt: nowIso(),
      updatedAt: nowIso(),
      messageCount: 0,
      isFallback: true,
      fallbackReason: reason,
    },
    messages: [],
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackRagAnswer(question: string, reason: string): RagAnswer {
  const trace: RagTraceRun = {
    id: 0,
    status: "fallback",
    startedAt: nowIso(),
    finishedAt: nowIso(),
    durationMs: 0,
    metadata: { question },
    nodes: [],
  };
  return {
    queryId: 0,
    provider: "mock",
    model: "mimo-v2.5-pro",
    status: "fallback",
    isMock: true,
    answer: `RAG 命令不可用：${reason}`,
    fallbackReason: reason,
    retrievedCount: 0,
    citations: [],
    trace,
    isFallback: true,
  };
}

function fallbackMimoStatus(reason: string): MimoStatus {
  return {
    provider: "mimo",
    extractModel: "mimo-v2.5",
    organizeModel: "mimo-v2.5-pro",
    hasKey: false,
    status: "missing_key",
    keyHasBom: false,
    keyNeedsCleanup: false,
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackArchiveMap(reason: string): ArchiveMapSnapshot {
  return {
    run: {
      id: 0,
      status: "fallback",
      startedAt: nowIso(),
      finishedAt: nowIso(),
      directoryCount: 0,
      fileCount: 0,
      historyCount: 0,
      markdownPath: ".thebrain/rules/archive-map.md",
      error: reason,
    },
    generatedAt: nowIso(),
    markdownPath: ".thebrain/rules/archive-map.md",
    directoryCount: 0,
    fileCount: 0,
    historyCount: 0,
    directories: [],
    markdown: "",
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackImportResults(sourcePaths: string[], mode: string, reason: string): InboxImportResult[] {
  return sourcePaths.map((sourcePath) => ({
    sourcePath,
    mode,
    status: "failed",
    error: reason,
    isFallback: true,
    fallbackReason: reason,
  }));
}

function fallbackExtract(relativePath: string, reason: string): MimoExtractResult {
  return {
    provider: "mock",
    model: "mimo-v2.5",
    status: "fallback",
    isMock: true,
    relativePath,
    text: "",
    error: reason,
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackQueueStatus(reason: string, overrides: Partial<QueueStatus> = {}): QueueStatus {
  return {
    state: "paused",
    active: 0,
    pending: 0,
    retrying: 0,
    failed: 0,
    conflicts: 0,
    completedToday: 0,
    concurrency: defaultAppSettings.queueConcurrency,
    retryLimit: defaultAppSettings.retryLimit,
    lastEvent: "后台队列命令尚未接入",
    updatedAt: nowIso(),
    items: { pending: [], running: [], failed: [], conflicts: [], completed: [] },
    isFallback: true,
    fallbackReason: reason,
    ...overrides,
  };
}

function fallbackWorkerStatus(reason: string): WorkerStatus {
  return {
    listener: {
      enabled: false,
      status: "paused",
      updatedAt: nowIso(),
      lastError: reason,
    },
    pending: 0,
    running: 0,
    failed: 0,
    conflicts: 0,
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackResidentWorkerStatus(reason: string): ResidentWorkerStatus {
  return {
    running: false,
    vaultPath: "",
    intervalMs: 5000,
    maxItemsPerTick: 1,
    stableWaitMs: 1000,
    lastStatus: "stopped",
    lastProcessed: 0,
    lastMoved: 0,
    lastFailed: 0,
    lastConflicts: 0,
    lastError: reason,
    updatedAt: nowIso(),
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackInboxListenerStatus(reason: string): InboxListenerStatus {
  return {
    enabled: false,
    status: "paused",
    lastEnqueuedCount: 0,
    lastError: reason,
    updatedAt: nowIso(),
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackWorkerRun(reason: string): WorkerRunResult {
  return {
    status: "failed",
    processed: 0,
    moved: 0,
    skipped: 0,
    failed: 1,
    conflicts: 0,
    items: [],
    listener: {
      enabled: false,
      status: "paused",
      updatedAt: nowIso(),
      lastError: reason,
    },
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackBudgetStatus(reason: string, settings = defaultAppSettings): BudgetStatus {
  return {
    provider: settings.aiDecisionModel,
    monthlyLimitCents: settings.budgetMonthlyCents,
    hardStopCents: settings.budgetHardStopCents,
    spentCents: 0,
    remainingCents: settings.budgetMonthlyCents,
    totalTokens: 0,
    exhausted: false,
    updatedAt: nowIso(),
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackOrganizePlan(reason: string): AiOrganizePlan {
  return {
    id: "fallback-plan",
    summary: "AI 整理命令尚未接入，当前显示前端占位计划。",
    candidates: [
      {
        id: "fallback-candidate-1",
        sourceRelativePath: "000-收集箱/示例笔记.md",
        targetRelativePath: "100-项目/示例笔记.md",
        confidence: 0.72,
        reason: "根据标题与内容关键词建议归入项目资料。",
        tags: ["项目", "待确认"],
        isFallback: true,
        fallbackReason: reason,
      },
    ],
    createdAt: nowIso(),
    isFallback: true,
    fallbackReason: reason,
  };
}

function fallbackTodoScheduleCandidates(reason: string): TodoScheduleCandidate[] {
  return [
    {
      id: "fallback-todo-1",
      kind: "todo",
      sourceRelativePath: "000-收集箱/示例笔记.md",
      title: "确认收集箱整理候选",
      excerpt: "前端占位候选，等待后端从 Markdown 中抽取 TODO。",
      confidence: 0.68,
      status: "pending",
      isFallback: true,
      fallbackReason: reason,
    },
    {
      id: "fallback-schedule-1",
      kind: "schedule",
      sourceRelativePath: "000-收集箱/会议记录.md",
      title: "复盘会议记录",
      excerpt: "前端占位日程，等待后端识别时间表达。",
      dueAt: nowIso(),
      confidence: 0.61,
      status: "pending",
      isFallback: true,
      fallbackReason: reason,
    },
  ];
}

function normalizeQueueStatus(raw: any): QueueStatus {
  if (!raw || typeof raw !== "object" || "state" in raw) {
    return raw as QueueStatus;
  }
  const pending = Array.isArray(raw.pending) ? raw.pending.map(normalizeQueueItem) : [];
  const running = Array.isArray(raw.running) ? raw.running.map(normalizeQueueItem) : [];
  const failed = Array.isArray(raw.failed) ? raw.failed.map(normalizeQueueItem) : [];
  const conflicts = Array.isArray(raw.conflicts) ? raw.conflicts.map(normalizeQueueItem) : [];
  const completed = Array.isArray(raw.completed) ? raw.completed.map(normalizeQueueItem) : [];
  const listenerStatus = raw.listener?.status ?? "idle";
  const state: QueueStatus["state"] =
    listenerStatus === "paused" || raw.listener?.enabled === false
      ? "paused"
      : listenerStatus === "watching"
        ? "running"
        : listenerStatus === "failed"
          ? "error"
          : "idle";
  return {
    state,
    active: running.length,
    pending: pending.length,
    retrying: 0,
    failed: failed.length,
    conflicts: conflicts.length,
    completedToday: completed.length,
    concurrency: 1,
    retryLimit: 3,
    lastEvent: raw.listener?.lastEvent ?? raw.listener?.lastEventAt ?? raw.listener?.status,
    updatedAt: raw.listener?.updatedAt ?? nowIso(),
    items: { pending, running, failed, conflicts, completed },
  };
}

function normalizeQueueItem(raw: any): QueueItem {
  if (!raw || typeof raw !== "object") {
    return raw as QueueItem;
  }
  return {
    id: Number(raw.id),
    kind: raw.kind ?? "",
    relativePath: raw.relativePath ?? raw.relative_path ?? "",
    status: raw.status ?? "",
    dedupeKey: raw.dedupeKey ?? raw.dedupe_key,
    payload: raw.payload ?? {},
    attempts: Number(raw.attempts ?? 0),
    maxAttempts: Number(raw.maxAttempts ?? raw.max_attempts ?? 0),
    lockedAt: raw.lockedAt ?? raw.locked_at,
    runAfter: raw.runAfter ?? raw.run_after,
    createdAt: raw.createdAt ?? raw.created_at ?? nowIso(),
    updatedAt: raw.updatedAt ?? raw.updated_at ?? nowIso(),
    isFallback: raw.isFallback,
    fallbackReason: raw.fallbackReason,
  };
}

function normalizeInboxListenerStatus(raw: any): InboxListenerStatus {
  if (!raw || typeof raw !== "object" || "lastEnqueuedCount" in raw) {
    return raw as InboxListenerStatus;
  }
  return {
    enabled: Boolean(raw.enabled),
    status: raw.status ?? "paused",
    watchPath: raw.watchPath,
    lastEvent: raw.lastEvent,
    lastEnqueuedCount: raw.lastEnqueuedCount ?? 0,
    lastEventAt: raw.lastEventAt,
    lastError: raw.lastError,
    updatedAt: raw.updatedAt ?? nowIso(),
    isFallback: raw.isFallback,
    fallbackReason: raw.fallbackReason,
  };
}

function normalizeBudgetStatus(raw: any): BudgetStatus {
  if (!raw || typeof raw !== "object" || "monthlyLimitCents" in raw) {
    return raw as BudgetStatus;
  }
  const monthlyLimitCents = raw.settings?.monthlyLimitCents ?? 0;
  const spentCents = raw.spentMonthCents ?? 0;
  return {
    provider: "mimo",
    monthlyLimitCents,
    hardStopCents: raw.settings?.dailyLimitCents ?? monthlyLimitCents,
    spentCents,
    remainingCents: Math.max(0, monthlyLimitCents - spentCents),
    totalTokens: raw.totalTokensToday ?? 0,
    exhausted: Boolean(raw.budgetExhausted),
    updatedAt: raw.settings?.updatedAt ?? nowIso(),
  };
}

function normalizeOrganizePlan(raw: any): AiOrganizePlan {
  if (raw?.plan) {
    const plan = raw.plan as OrganizeDecisionResult;
    return {
      id: `${plan.provider ?? "mimo"}-${Date.now()}`,
      summary:
        plan.summary ||
        plan.reason ||
        `${raw.extraction?.status ?? "unknown"} extraction -> ${plan.status ?? "unknown"} plan`,
      candidates: [
        {
          id: plan.sourceRelativePath,
          sourceRelativePath: plan.sourceRelativePath,
          targetRelativePath: plan.targetRelativePath,
          confidence: plan.confidence ?? 0,
          reason: plan.reason ?? plan.error ?? "",
          tags: plan.tags ?? [],
          isFallback: plan.isMock || raw.extraction?.isMock,
          fallbackReason: plan.error || raw.extraction?.error,
        },
      ],
      createdAt: nowIso(),
      isFallback: plan.isMock || raw.extraction?.isMock,
      fallbackReason: plan.error || raw.extraction?.error,
    };
  }
  if (!raw || typeof raw !== "object" || "candidates" in raw) {
    return raw as AiOrganizePlan;
  }
  return {
    id: `${raw.provider ?? "mimo"}-${Date.now()}`,
    summary: raw.summary ?? raw.reason ?? "整理计划已生成",
    candidates: [
      {
        id: raw.sourceRelativePath ?? "candidate-1",
        sourceRelativePath: raw.sourceRelativePath,
        targetRelativePath: raw.targetRelativePath,
        confidence: raw.confidence ?? 0,
        reason: raw.reason ?? "",
        tags: raw.tags ?? [],
      },
    ],
    createdAt: nowIso(),
    isFallback: raw.isMock,
    fallbackReason: raw.error,
  };
}

function normalizeMoveResult(raw: any): MoveResult {
  if (!raw || typeof raw !== "object" || "moved" in raw) {
    return raw as MoveResult;
  }
  return {
    sourceRelativePath: raw.sourceRelativePath,
    targetRelativePath: raw.targetRelativePath,
    moved: raw.status === "moved",
    auditId: String(raw.id),
    message: raw.status === "moved" ? "文件已移动并记录日志" : raw.status,
  };
}

function normalizeRollbackResult(raw: any): RollbackResult {
  if (!raw || typeof raw !== "object" || "rolledBack" in raw) {
    return raw as RollbackResult;
  }
  return {
    auditId: String(raw.id),
    rolledBack: raw.status === "rolled_back",
    restoredRelativePath: raw.sourceRelativePath,
    message: raw.status === "rolled_back" ? "文件已回滚到收集箱" : raw.status,
  };
}

function normalizeMoveLog(raw: any): MoveLog {
  if (!raw || typeof raw !== "object") {
    return raw as MoveLog;
  }
  return {
    id: Number(raw.id),
    operation: raw.operation ?? "",
    sourceRelativePath: raw.sourceRelativePath ?? raw.source_relative_path ?? "",
    targetRelativePath: raw.targetRelativePath ?? raw.target_relative_path ?? "",
    reason: raw.reason,
    status: raw.status ?? "",
    createdAt: raw.createdAt ?? raw.created_at ?? nowIso(),
    movedAt: raw.movedAt ?? raw.moved_at,
    rolledBackAt: raw.rolledBackAt ?? raw.rolled_back_at,
    error: raw.error,
    isFallback: raw.isFallback,
    fallbackReason: raw.fallbackReason,
  };
}

function normalizeAuditEvent(raw: any): AuditEvent {
  if (!raw || typeof raw !== "object") {
    return raw as AuditEvent;
  }
  const payload = raw.payload && typeof raw.payload === "object" ? raw.payload : {};
  return {
    id: Number(raw.id),
    eventType: raw.eventType ?? raw.event_type ?? "",
    payload,
    createdAt: raw.createdAt ?? raw.created_at ?? nowIso(),
    isFallback: raw.isFallback,
    fallbackReason: raw.fallbackReason,
  };
}

function normalizeRagConversation(raw: any): RagConversation {
  if (!raw || typeof raw !== "object") {
    return raw as RagConversation;
  }
  return {
    id: Number(raw.id ?? raw.conversationId ?? raw.conversation_id ?? 0),
    title: raw.title ?? raw.name ?? "未命名 RAG 对话",
    createdAt: raw.createdAt ?? raw.created_at ?? nowIso(),
    updatedAt: raw.updatedAt ?? raw.updated_at ?? raw.lastMessageAt ?? raw.last_message_at ?? nowIso(),
    lastMessageAt: raw.lastMessageAt ?? raw.last_message_at,
    messageCount: raw.messageCount ?? raw.message_count,
    isFallback: raw.isFallback,
    fallbackReason: raw.fallbackReason,
  };
}

function normalizeRagMessage(raw: any): RagMessage {
  if (!raw || typeof raw !== "object") {
    return raw as RagMessage;
  }
  return {
    id: Number(raw.id ?? 0),
    conversationId: Number(raw.conversationId ?? raw.conversation_id ?? 0),
    role: raw.role ?? "assistant",
    content: raw.content ?? raw.message ?? raw.answer ?? "",
    createdAt: raw.createdAt ?? raw.created_at ?? nowIso(),
    queryId: raw.queryId ?? raw.query_id,
    status: raw.status,
    isMock: raw.isMock ?? raw.is_mock,
    fallbackReason: raw.fallbackReason ?? raw.fallback_reason,
    isFallback: raw.isFallback ?? raw.is_fallback,
  };
}

function normalizeRagConversationDetail(raw: any): RagConversationDetail {
  const conversation = normalizeRagConversation(raw?.conversation ?? raw ?? {});
  return {
    conversation,
    messages: Array.isArray(raw?.messages) ? raw.messages.map(normalizeRagMessage) : [],
    isFallback: raw?.isFallback ?? conversation.isFallback,
    fallbackReason: raw?.fallbackReason ?? conversation.fallbackReason,
  };
}

function normalizeBatchQueueResult(raw: any): BatchQueueResult {
  return {
    requested: Number(raw?.requested ?? 0),
    succeeded: Array.isArray(raw?.succeeded) ? raw.succeeded.map(normalizeQueueItem) : [],
    failed: Array.isArray(raw?.failed)
      ? raw.failed.map((item: any) => ({ id: Number(item.id), message: String(item.message ?? "") }))
      : [],
    isFallback: raw?.isFallback,
    fallbackReason: raw?.fallbackReason,
  };
}

function normalizeBatchRollbackResult(raw: any): BatchRollbackResult {
  return {
    requested: Number(raw?.requested ?? 0),
    rolledBack: Array.isArray(raw?.rolledBack ?? raw?.rolled_back)
      ? (raw.rolledBack ?? raw.rolled_back).map(normalizeMoveLog)
      : [],
    failed: Array.isArray(raw?.failed)
      ? raw.failed.map((item: any) => ({ id: Number(item.id), message: String(item.message ?? "") }))
      : [],
    isFallback: raw?.isFallback,
    fallbackReason: raw?.fallbackReason,
  };
}

function normalizeCandidate(raw: any): TodoScheduleCandidate {
  if (!raw || typeof raw !== "object" || "kind" in raw) {
    return raw as TodoScheduleCandidate;
  }
  return {
    id: String(raw.id),
    kind: raw.candidateType === "schedule" ? "schedule" : "todo",
    sourceRelativePath: raw.sourceRelativePath ?? "",
    title: raw.title,
    excerpt: typeof raw.payload === "string" ? raw.payload : JSON.stringify(raw.payload ?? {}),
    dueAt: raw.payload?.dueAt ?? raw.payload?.date,
    confidence: raw.payload?.confidence ?? 0.5,
    status: raw.status === "rejected" ? "dismissed" : raw.status,
  };
}

function normalizeConflict(raw: any): ConflictItem {
  if (!raw || typeof raw !== "object" || "options" in raw) {
    return raw as ConflictItem;
  }
  const payload = raw.payload ?? raw;
  const source =
    raw.sourceRelativePath ?? payload.sourceRelativePath ?? payload.sourcePath ?? payload.relativePath ?? "";
  const target = raw.targetRelativePath ?? payload.targetRelativePath ?? payload.relativePath ?? "";
  const rawRecommendations = raw.recommendations ?? payload.recommendedRules ?? [];
  const recommendations = Array.isArray(rawRecommendations) ? rawRecommendations : [];
  const rawRenameSuggestions = raw.renameSuggestions ?? payload.renameSuggestions ?? payload.suggestedTargets ?? [];
  const renameSuggestions = Array.isArray(rawRenameSuggestions)
    ? rawRenameSuggestions
        .map((item: any) =>
          typeof item === "string"
            ? { targetRelativePath: item }
            : {
                targetRelativePath: item?.targetRelativePath ?? item?.relativePath ?? item?.path ?? "",
                reason: item?.reason,
                available: item?.available ?? (item?.exists === undefined ? undefined : !item.exists),
              },
        )
        .filter((item: ConflictRenameSuggestion) => item.targetRelativePath)
    : [];
  const preview = raw.preview ?? raw.context ?? payload.preview ?? payload.context;
  return {
    id: String(raw.id ?? raw.eventId ?? payload.id ?? Date.now()),
    eventId: raw.eventId ?? payload.eventId,
    kind: raw.kind ?? payload.kind ?? "target_exists",
    createdAt: raw.createdAt,
    status: raw.status ?? payload.status ?? "open",
    sourceRelativePath: source,
    targetRelativePath: target,
    message: raw.message ?? payload.message ?? payload.error ?? payload.reason ?? "Conflict requires manual handling",
    options: Array.isArray(raw.options ?? payload.options) ? raw.options ?? payload.options : ["rename", "skip", "keep_existing"],
    recommendedAction: raw.recommendedAction ?? payload.recommendedAction ?? "rename",
    recommendations,
    preview,
    context: raw.context ?? payload.context,
    renameSuggestions,
    payload,
    isFallback: raw.isFallback,
    fallbackReason: raw.fallbackReason,
  };
}

function normalizeConflictRule(raw: any): ConflictRule {
  return {
    id: Number(raw?.id ?? 0),
    ruleKey: raw?.ruleKey ?? raw?.rule_key ?? "",
    sourcePattern: raw?.sourcePattern ?? raw?.source_pattern ?? "",
    targetPattern: raw?.targetPattern ?? raw?.target_pattern ?? "",
    answer: raw?.answer ?? "",
    action: raw?.action ?? "rename",
    autoApply: Boolean(raw?.autoApply ?? raw?.auto_apply),
    matchSummary: raw?.matchSummary ?? raw?.match_summary ?? "",
    markdownPath: raw?.markdownPath ?? raw?.markdown_path ?? "",
    status: raw?.status ?? "active",
    createdAt: raw?.createdAt ?? raw?.created_at ?? nowIso(),
    updatedAt: raw?.updatedAt ?? raw?.updated_at ?? nowIso(),
    hitCount: Number(raw?.hitCount ?? raw?.hit_count ?? 0),
  };
}

function normalizeConflictRuleUpdate(raw: any): ConflictRuleUpdateResult {
  return {
    rule: normalizeConflictRule(raw?.rule ?? {}),
    auditId: Number(raw?.auditId ?? raw?.audit_id ?? 0),
    status: raw?.status ?? "updated",
    isFallback: raw?.isFallback,
    fallbackReason: raw?.fallbackReason,
  };
}

function normalizeConflictResolution(raw: any, conflictId: string, action: ConflictResolutionAction): ConflictResolutionResult {
  if (!raw || typeof raw !== "object" || "resolved" in raw) {
    return raw as ConflictResolutionResult;
  }
  return {
    conflictId,
    action,
    resolved: raw.status === "resolved",
    message: raw.status ?? "resolved",
  };
}

function normalizeSticky(raw: any): StickyNote {
  if (!raw || typeof raw !== "object" || "content" in raw) {
    return raw as StickyNote;
  }
  return {
    id: String(raw.id),
    title: raw.title,
    content: raw.body,
    autosavePath: `000-收集箱/便利贴-${raw.id}.md`,
    pinned: Boolean(raw.pinned),
    updatedAt: raw.updatedAt ?? nowIso(),
  };
}

export async function selectVault(): Promise<string | null> {
  try {
    const fromCommand = await invoke<string | null>("select_vault");
    if (fromCommand) {
      return fromCommand;
    }
  } catch {
    // During browser-only Vite preview, fall through to the dialog plugin call.
  }

  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function selectImportFiles(): Promise<string[]> {
  const selected = await open({ directory: false, multiple: true });
  if (Array.isArray(selected)) return selected.map(String);
  return typeof selected === "string" ? [selected] : [];
}

export const commands = {
  initVault: (vaultPath: string) => invoke<VaultInitResult>("init_vault", { vaultPath }),
  listVaultTree: (vaultPath: string) => invoke<VaultTreeNode[]>("list_vault_tree", { vaultPath }),
  listInbox: (vaultPath: string) => invoke<InboxItem[]>("list_inbox", { vaultPath }),
  parseInboxLedger: (vaultPath: string) =>
    invoke<LedgerItem[]>("parse_inbox_ledger", { vaultPath }),
  getArchiveMap: (vaultPath: string) =>
    invokeWithFallback<ArchiveMapSnapshot>("get_archive_map", { vaultPath }, fallbackArchiveMap),
  rebuildArchiveMap: (vaultPath: string) =>
    invokeWithFallback<ArchiveMapSnapshot>("rebuild_archive_map", { vaultPath }, fallbackArchiveMap),
  importToInbox: (vaultPath: string, sourcePaths: string[], mode: "copy" | "move" = "copy") =>
    invokeWithFallback<InboxImportResult[]>(
      "import_to_inbox",
      { vaultPath, sourcePaths, mode },
      (reason) => fallbackImportResults(sourcePaths, mode, reason),
    ),
  getAiUsage: (vaultPath: string) =>
    invokeWithFallback<UsageSummary>("get_ai_usage", { vaultPath }, fallbackUsage),
  rebuildRagIndex: (vaultPath: string) =>
    invokeWithFallback<RagIndexRun & CommandMeta>(
      "rebuild_rag_index",
      { vaultPath },
      fallbackRagRun,
    ),
  getRagIndexStatus: (vaultPath: string) =>
    invokeWithFallback<RagIndexStatus>(
      "get_rag_index_status",
      { vaultPath },
      fallbackRagStatus,
    ),
  listRagConversations: (vaultPath: string, limit = 20) =>
    invokeWithFallback<any[]>(
      "list_rag_conversations",
      { vaultPath, limit },
      fallbackRagConversations,
    ).then((items) => items.map(normalizeRagConversation)),
  createRagConversation: (vaultPath: string, title?: string) =>
    invokeWithFallback<any>(
      "create_rag_conversation",
      { vaultPath, title },
      (reason) => fallbackRagConversation(title, reason),
    ).then(normalizeRagConversation),
  getRagConversation: (vaultPath: string, conversationId: number) =>
    invokeWithFallback<any>(
      "get_rag_conversation",
      { vaultPath, conversationId },
      (reason) => fallbackRagConversationDetail(conversationId, reason),
    ).then(normalizeRagConversationDetail),
  askRag: (
    vaultPath: string,
    question: string,
    topK = 6,
    conversationId?: number,
    scope?: RagScope,
  ) =>
    invokeWithFallback<RagAnswer>(
      "ask_rag",
      { vaultPath, question, topK, conversationId, scope },
      (reason) => fallbackRagAnswer(question, reason),
    ),
  getLatestRagTrace: (vaultPath: string) =>
    invokeWithFallback<RagTraceRun | null>(
      "get_latest_rag_trace",
      { vaultPath },
      () => null,
    ),
  readMarkdown: (vaultPath: string, relativePath: string) =>
    invoke<MarkdownDocument>("read_markdown", { vaultPath, relativePath }),
  saveMarkdown: (
    vaultPath: string,
    relativePath: string,
    content: string,
    frontmatter: unknown,
  ) =>
    invoke<MarkdownDocument>("save_markdown", {
      vaultPath,
      relativePath,
      content,
      frontmatter,
    }),
  exportMarkdown: (vaultPath: string, relativePath: string) =>
    invoke<MarkdownExport>("export_markdown", { vaultPath, relativePath }),
  getAppSettings: (vaultPath: string) =>
    invokeWithFallback<AppSettings>("get_app_settings", { vaultPath }, (reason) => ({
      ...defaultAppSettings,
      isFallback: true,
      fallbackReason: reason,
    })),
  saveAppSettings: (vaultPath: string, settings: AppSettings) =>
    invokeWithFallback<AppSettings>("save_app_settings", { vaultPath, settings }, (reason) => ({
      ...settings,
      isFallback: true,
      fallbackReason: reason,
    })),
  registerGlobalShortcut: (shortcut = "Ctrl+Space") =>
    invokeWithFallback<string>("register_global_shortcut", { shortcut }, () => shortcut),
  prewarmStickyWindows: (count: number) =>
    invokeWithFallback<string[]>("prewarm_sticky_windows", { count }, () => []),
  getQueueStatus: (vaultPath: string) =>
    invokeWithFallback<any>("get_queue_status", { vaultPath }, fallbackQueueStatus).then(normalizeQueueStatus),
  getInboxListenerStatus: (vaultPath: string) =>
    invokeWithFallback<any>(
      "get_inbox_listener_status",
      { vaultPath },
      fallbackInboxListenerStatus,
    ).then(normalizeInboxListenerStatus),
  startInboxListener: (vaultPath: string) =>
    invokeWithFallback<any>("start_inbox_watcher", { vaultPath }, (reason) =>
      fallbackQueueStatus(reason, { state: "running", lastEvent: "监听启动命令不可用" }),
    ).then(normalizeQueueStatus),
  stopInboxListener: (vaultPath: string) =>
    invokeWithFallback<any>("stop_inbox_watcher", { vaultPath }, (reason) =>
      fallbackQueueStatus(reason, { state: "paused", lastEvent: "监听停止命令不可用" }),
    ).then(normalizeQueueStatus),
  getWorkerStatus: (vaultPath: string) =>
    invokeWithFallback<WorkerStatus>("get_worker_status", { vaultPath }, fallbackWorkerStatus),
  getResidentWorkerStatus: (vaultPath: string) =>
    invokeWithFallback<ResidentWorkerStatus>(
      "get_resident_worker_status",
      { vaultPath },
      fallbackResidentWorkerStatus,
    ),
  startResidentWorker: (vaultPath: string) =>
    invokeWithFallback<ResidentWorkerStatus>(
      "start_resident_worker",
      { vaultPath, options: { intervalMs: 5000, maxItemsPerTick: 1, stableWaitMs: 1000 } },
      fallbackResidentWorkerStatus,
    ),
  stopResidentWorker: (vaultPath: string) =>
    invokeWithFallback<ResidentWorkerStatus>(
      "stop_resident_worker",
      { vaultPath },
      fallbackResidentWorkerStatus,
    ),
  runInboxWorker: (vaultPath: string, maxItems = 8) =>
    invokeWithFallback<WorkerRunResult>(
      "run_inbox_worker",
      { vaultPath, options: { maxItems, stableWaitMs: 1000, forceMock: false } },
      fallbackWorkerRun,
    ),
  pauseInboxWorker: (vaultPath: string) =>
    invokeWithFallback<WorkerStatus>("pause_inbox_worker", { vaultPath }, fallbackWorkerStatus),
  resumeInboxWorker: (vaultPath: string) =>
    invokeWithFallback<WorkerStatus>("resume_inbox_worker", { vaultPath }, fallbackWorkerStatus),
  scanInboxQueue: (vaultPath: string) =>
    invokeWithFallback<any>("scan_inbox_queue", { vaultPath }, (reason) =>
      fallbackQueueStatus(reason, { lastEvent: "扫描入队命令不可用" }),
    ).then(normalizeQueueStatus),
  pauseQueue: (vaultPath: string) =>
    invokeWithFallback<any>("stop_inbox_watcher", { vaultPath }, (reason) =>
      fallbackQueueStatus(reason, { state: "paused", lastEvent: "队列已在前端标记为暂停" }),
    ).then(normalizeQueueStatus),
  resumeQueue: (vaultPath: string) =>
    invokeWithFallback<any>("start_inbox_watcher", { vaultPath }, (reason) =>
      fallbackQueueStatus(reason, { state: "running", lastEvent: "队列已在前端标记为运行" }),
    ).then(normalizeQueueStatus),
  retryQueueItem: (vaultPath: string, queueId: number) =>
    invokeWithFallback<any>(
      "retry_queue_item",
      { vaultPath, queueId },
      (reason) => ({
        id: queueId,
        kind: "",
        relativePath: "",
        status: "failed",
        payload: {},
        attempts: 0,
        maxAttempts: 0,
        createdAt: nowIso(),
        updatedAt: nowIso(),
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeQueueItem),
  skipQueueItem: (vaultPath: string, queueId: number, reason = "skipped from inbox recovery panel") =>
    invokeWithFallback<any>(
      "skip_queue_item",
      { vaultPath, queueId, reason },
      (fallbackReason) => ({
        id: queueId,
        kind: "",
        relativePath: "",
        status: "failed",
        payload: {},
        attempts: 0,
        maxAttempts: 0,
        createdAt: nowIso(),
        updatedAt: nowIso(),
        isFallback: true,
        fallbackReason,
      }),
    ).then(normalizeQueueItem),
  retryQueueItems: (vaultPath: string, queueIds: number[]) =>
    invokeWithFallback<any>(
      "retry_queue_items",
      { vaultPath, queueIds },
      (reason) => ({
        requested: queueIds.length,
        succeeded: [],
        failed: queueIds.map((id) => ({ id, message: reason })),
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeBatchQueueResult),
  skipQueueItems: (vaultPath: string, queueIds: number[], reason = "batch skipped from inbox recovery panel") =>
    invokeWithFallback<any>(
      "skip_queue_items",
      { vaultPath, queueIds, reason },
      (fallbackReason) => ({
        requested: queueIds.length,
        succeeded: [],
        failed: queueIds.map((id) => ({ id, message: fallbackReason })),
        isFallback: true,
        fallbackReason,
      }),
    ).then(normalizeBatchQueueResult),
  getBudgetStatus: (vaultPath: string) =>
    invokeWithFallback<any>("get_budget_status", { vaultPath }, fallbackBudgetStatus).then(normalizeBudgetStatus),
  saveBudgetSettings: (vaultPath: string, settings: BudgetSettings) =>
    invokeWithFallback<any>("save_budget_settings", { vaultPath, settings }, (reason) =>
      fallbackBudgetStatus(reason, {
        ...defaultAppSettings,
        budgetMonthlyCents: settings.monthlyLimitCents,
        budgetHardStopCents: settings.hardStopCents,
      }),
    ).then(normalizeBudgetStatus),
  getMimoStatus: (vaultPath: string) =>
    invokeWithFallback<MimoStatus>("get_mimo_status", { vaultPath }, fallbackMimoStatus),
  saveMimoApiKey: (vaultPath: string, apiKey: string) =>
    invokeWithFallback<MimoStatus>(
      "save_mimo_api_key",
      { vaultPath, input: { apiKey } },
      fallbackMimoStatus,
    ),
  extractWithMimo: (vaultPath: string, relativePath: string, forceMock = false) =>
    invokeWithFallback<MimoExtractResult>(
      "extract_with_mimo",
      { vaultPath, input: { relativePath, forceMock } },
      (reason) => fallbackExtract(relativePath, reason),
    ),
  planInboxItem: (vaultPath: string, sourceRelativePath: string, forceMock = false) =>
    invokeWithFallback<any>(
      "plan_inbox_item",
      { vaultPath, sourceRelativePath, forceMock },
      (reason) => ({
        extraction: fallbackExtract(sourceRelativePath, reason),
        plan: {
          provider: "mock",
          model: "mimo-v2.5-pro",
          status: "fallback",
          isMock: true,
          sourceRelativePath,
          targetRelativePath: sourceRelativePath.replace(/^000-[^/]+\//, "100-Organized/"),
          tags: ["inbox"],
          summary: "Fallback organization decision.",
          reason,
          confidence: 0.35,
          todoCandidates: [],
          scheduleCandidates: [],
          error: reason,
        },
        candidates: [],
        budget: fallbackBudgetStatus(reason),
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then((raw) => ({
      ...raw,
      candidates: (raw.candidates ?? []).map(normalizeCandidate),
      budget: normalizeBudgetStatus(raw.budget),
    }) as InboxPlanResult),
  planAiOrganize: (
    vaultPath: string,
    sourceRelativePath?: string,
    extractedText?: string,
    forceMock = false,
  ) =>
    invokeWithFallback<any>(
      "plan_ai_organize",
      { vaultPath, sourceRelativePath, extractedText, forceMock },
      fallbackOrganizePlan,
    ).then(normalizeOrganizePlan),
  runAiOrganize: (
    vaultPath: string,
    planId?: string,
    sourceRelativePath?: string,
    targetRelativePath?: string,
    reason?: string,
  ) =>
    invokeWithFallback<AiOrganizeResult>(
      "run_ai_organize",
      { vaultPath, planId, sourceRelativePath, targetRelativePath, reason },
      (reason) => ({
        moved: 0,
        skipped: 1,
        conflicts: 0,
        auditId: "fallback-audit",
        message: "AI 整理命令尚未接入，未移动任何文件。",
        isFallback: true,
        fallbackReason: reason,
      }),
    ),
  moveInboxItem: (
    vaultPath: string,
    sourceRelativePath: string,
    targetRelativePath: string,
    overwrite = false,
  ) =>
    invokeWithFallback<any>(
      "move_inbox_item",
      { vaultPath, sourceRelativePath, targetRelativePath, reason: overwrite ? "overwrite requested" : undefined },
      (reason) => ({
        sourceRelativePath,
        targetRelativePath,
        moved: false,
        auditId: "fallback-move",
        message: "移动命令尚未接入，前端仅保留本次操作意图。",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeMoveResult),
  rollbackMove: (vaultPath: string, auditId?: string, sourceRelativePath?: string) =>
    invokeWithFallback<any>(
      "rollback_move",
      { vaultPath, movementId: auditId ? Number(auditId) : undefined, auditId, sourceRelativePath },
      (reason) => ({
        auditId,
        rolledBack: false,
        restoredRelativePath: sourceRelativePath,
        message: "回滚命令尚未接入，未修改文件。",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeRollbackResult),
  listMoveLogs: (vaultPath: string) =>
    invokeWithFallback<any[]>("list_move_logs", { vaultPath }, () => []).then((items) =>
      items.map(normalizeMoveLog),
    ),
  rollbackMoves: (vaultPath: string, movementIds: number[]) =>
    invokeWithFallback<any>(
      "rollback_moves",
      { vaultPath, movementIds },
      (reason) => ({
        requested: movementIds.length,
        rolledBack: [],
        failed: movementIds.map((id) => ({ id, message: reason })),
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeBatchRollbackResult),
  listAuditEvents: (vaultPath: string, limit = 30, eventType?: string) =>
    invokeWithFallback<any[]>(
      "list_audit_events",
      { vaultPath, limit, eventType },
      (reason) => [
        {
          id: 0,
          eventType: "fallback",
          payload: { message: reason },
          createdAt: nowIso(),
          isFallback: true,
          fallbackReason: reason,
        },
      ],
    ).then((items) => items.map(normalizeAuditEvent)),
  listTodoScheduleCandidates: (vaultPath: string) =>
    invokeWithFallback<any[]>(
      "list_todo_schedule_candidates",
      { vaultPath },
      fallbackTodoScheduleCandidates,
    ).then((items) => items.map(normalizeCandidate)),
  confirmTodoScheduleCandidate: (
    vaultPath: string,
    candidateId: string,
    patch?: Partial<TodoScheduleCandidate>,
  ) =>
    invokeWithFallback<any>(
      "confirm_todo_schedule_candidate",
      { vaultPath, candidateId: Number(candidateId), patch },
      (reason) => ({
        id: candidateId,
        kind: patch?.kind ?? "todo",
        sourceRelativePath: patch?.sourceRelativePath ?? "",
        title: patch?.title ?? "已确认候选",
        excerpt: patch?.excerpt ?? "",
        dueAt: patch?.dueAt,
        confidence: patch?.confidence ?? 1,
        status: "confirmed",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeCandidate),
  dismissTodoScheduleCandidate: (vaultPath: string, candidateId: string) =>
    invokeWithFallback<any>(
      "dismiss_todo_schedule_candidate",
      { vaultPath, candidateId: Number(candidateId) },
      (reason) => ({
        id: candidateId,
        kind: "todo",
        sourceRelativePath: "",
        title: "已忽略候选",
        excerpt: "",
        confidence: 0,
        status: "dismissed",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeCandidate),
  listStickyNotes: (vaultPath: string) =>
    invokeWithFallback<any[]>("list_sticky_notes", { vaultPath }, () => []).then((items) =>
      items.map(normalizeSticky),
    ),
  saveStickyNote: (vaultPath: string, note: StickyNote) =>
    invokeWithFallback<any>("save_sticky_note", {
      vaultPath,
      note: {
        id: Number.isFinite(Number(note.id)) ? Number(note.id) : undefined,
        title: note.title,
        body: note.content,
        color: "#fff59d",
        x: 80,
        y: 80,
        width: 320,
        height: 220,
        pinned: note.pinned,
        archived: false,
      },
    }, (reason) => ({
      ...note,
      updatedAt: nowIso(),
      isFallback: true,
      fallbackReason: reason,
    })).then(normalizeSticky),
  deleteStickyNote: (vaultPath: string, noteId: string) =>
    invokeWithFallback<{ noteId: string; isFallback?: boolean; fallbackReason?: string }>(
      "delete_sticky_note",
      { vaultPath, noteId: Number(noteId) },
      (reason) => ({ noteId, isFallback: true, fallbackReason: reason }),
    ),
  listConflicts: (vaultPath: string) =>
    invokeWithFallback<any[]>("list_conflicts", { vaultPath }, () => []).then((items) =>
      items.map(normalizeConflict),
    ),
  getConflictDetail: (vaultPath: string, conflictId: string) =>
    invokeWithFallback<any>(
      "get_conflict_detail",
      { vaultPath, conflictId },
      (reason) => ({
        id: conflictId,
        message: "conflict detail command is not available",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeConflict),
  submitConflictAnswer: (vaultPath: string, input: ConflictAnswerInput) =>
    invokeWithFallback<ConflictAnswerResult>(
      "submit_conflict_answer",
      { vaultPath, input },
      (reason) => ({
        conflictId: input.conflictId,
        rule: {
          id: 0,
          ruleKey: "fallback",
          sourcePattern: "",
          targetPattern: input.targetRelativePath ?? "",
          answer: input.answer,
          action: input.action ?? "rename",
          autoApply: input.autoApply ?? false,
          matchSummary: input.matchSummary ?? "",
          markdownPath: ".thebrain/rules/inbox-organizing-rules.md",
          status: "fallback",
          createdAt: nowIso(),
          updatedAt: nowIso(),
          hitCount: 0,
        },
        markdownPath: ".thebrain/rules/inbox-organizing-rules.md",
        auditId: 0,
        status: "fallback",
        isFallback: true,
        fallbackReason: reason,
      }),
    ),
  matchConflictRules: (
    vaultPath: string,
    sourceRelativePath: string,
    targetRelativePath: string,
    message?: string,
  ) =>
    invokeWithFallback<ConflictRuleMatch[]>(
      "match_conflict_rules",
      { vaultPath, sourceRelativePath, targetRelativePath, message },
      () => [],
    ),
  suggestConflictRenameTargets: (vaultPath: string, targetRelativePath: string, limit = 5) =>
    invokeWithFallback<any[]>(
      "suggest_conflict_rename_targets",
      { vaultPath, targetRelativePath, limit },
      () => [],
    ).then((items) =>
      items
        .map((item) => ({
          targetRelativePath: item?.targetRelativePath ?? item?.target_relative_path ?? "",
          available: item?.available ?? (item?.exists === undefined ? undefined : !item.exists),
        }))
        .filter((item) => item.targetRelativePath),
    ),
  listConflictRules: (vaultPath: string, includeDisabled = true) =>
    invokeWithFallback<any[]>(
      "list_conflict_rules",
      { vaultPath, includeDisabled },
      () => [],
    ).then((items) => items.map(normalizeConflictRule)),
  setConflictRuleStatus: (vaultPath: string, ruleId: number, status: string) =>
    invokeWithFallback<any>(
      "set_conflict_rule_status",
      { vaultPath, ruleId, status },
      (reason) => ({
        rule: { id: ruleId, status },
        auditId: 0,
        status: "fallback",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeConflictRuleUpdate),
  updateConflictRule: (vaultPath: string, input: ConflictRuleUpdateInput) =>
    invokeWithFallback<any>(
      "update_conflict_rule",
      { vaultPath, input },
      (reason) => ({
        rule: { id: input.ruleId, ...input },
        auditId: 0,
        status: "fallback",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then(normalizeConflictRuleUpdate),
  applyConflictRule: (vaultPath: string, conflictId: string, ruleId: number) =>
    invokeWithFallback<ApplyConflictRuleResult>(
      "apply_conflict_rule",
      { vaultPath, conflictId, ruleId },
      (reason) => ({
        conflictId,
        ruleId,
        status: "fallback",
        moved: false,
        message: "apply conflict rule command is not available",
        isFallback: true,
        fallbackReason: reason,
      }),
    ),
  resolveConflict: (
    vaultPath: string,
    conflictId: string,
    action: ConflictResolutionAction,
  ) =>
    invokeWithFallback<ConflictResolutionResult>(
      "resolve_conflict",
      { vaultPath, conflictId, action },
      (reason) => ({
        conflictId,
        action,
        resolved: false,
        message: "resolve_conflict command is not available; no file was changed and the conflict remains open.",
        isFallback: true,
        fallbackReason: reason,
      }),
    ).then((raw) => normalizeConflictResolution(raw, conflictId, action)),
};
