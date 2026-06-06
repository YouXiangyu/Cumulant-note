import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Activity,
  Archive,
  ArrowRight,
  Bot,
  Calendar,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Circle,
  CircleDollarSign,
  ClipboardCheck,
  Clock3,
  CloudUpload,
  Code2,
  Database,
  Download,
  Eye,
  File,
  FileAudio,
  FileImage,
  FileText,
  Folder,
  FolderOpen,
  HardDrive,
  Home,
  ImageIcon,
  Inbox,
  Keyboard,
  LayoutDashboard,
  Lightbulb,
  Link2,
  ListChecks,
  Mail,
  MessageCircle,
  MessageSquare,
  MoreHorizontal,
  Paperclip,
  Pause,
  Pencil,
  Pin,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Search,
  Send,
  Settings,
  Shield,
  Sparkles,
  Star,
  StickyNote,
  Target,
  Trash2,
  Upload,
  UserRound,
  Wand2,
  X,
  Zap,
} from "lucide-react";
import DOMPurify from "dompurify";
import { marked } from "marked";
import {
  AiOrganizePlan,
  AiOrganizeResult,
  ArchiveMapDirectory,
  ArchiveMapSnapshot,
  AppSettings,
  AuditEvent,
  AuditSearchQuery,
  BatchQueueRecoveryPreview,
  BatchRollbackPreview,
  BudgetStatus,
  commands,
  ConflictDiffLine,
  ConflictItem,
  ConflictResolutionAction,
  ConflictRule,
  defaultAppSettings,
  InboxListenerStatus,
  InboxItem,
  InboxImportResult,
  InboxPlanResult,
  LedgerItem,
  MarkdownDocument,
  MimoExtractResult,
  MimoStatus,
  MoveLog,
  OrganizeDecisionMetadata,
  QueueItem,
  QueueStatus,
  RagAnswer,
  RagConversation,
  RagIndexRun,
  RagIndexStatus,
  RagMessage,
  RagScope,
  RagTraceRun,
  RecentFileSummary,
  ResidentWorkerStatus,
  selectImportFiles,
  selectVault,
  StickyNote as StickyNoteRecord,
  TodoScheduleCandidate,
  TodoScheduleItem,
  UsageSummary,
  VaultInitResult,
  VaultTreeNode,
  WorkspaceInsights,
  WorkerRunResult,
  WorkerStatus,
} from "./api";

type ViewId = "dashboard" | "inbox" | "markdown" | "notes" | "personal" | "project" | "settings";
type ImportBehavior = "copy" | "move";
type RagScopeKind = RagScope["kind"];
type BatchRecoveryPreviewState =
  | { kind: "queue-retry" | "queue-skip"; preview: BatchQueueRecoveryPreview }
  | { kind: "rollback"; preview: BatchRollbackPreview };

interface AppError {
  title: string;
  detail: string;
  recovery: string;
}

interface ArchiveRuleDraft {
  userNote: string;
  organizingHint: string;
  locked: boolean;
}

interface ProjectItem {
  id: string;
  name: string;
  relativePath: string;
  updatedAt: string;
  count: number;
  fileCount: number;
  markdownCount: number;
  totalBytes: number;
  recentFiles: RecentFileSummary[];
  source: "insights" | "tree" | "fallback";
}

const defaultRelativePath = "000-收集箱/新笔记.md";
const settingsStorageKey = "thebrain.settings";
const notesStorageKey = "thebrain.stickyNotes";
// Frontend concept fallback: used only when the Vault has no project-like directories.
const fallbackProjects: ProjectItem[] = [
  { id: "ai-exam", name: "人工智能-期末考试", relativePath: "100-学校/人工智能-期末考试", updatedAt: "占位", count: 12, fileCount: 12, markdownCount: 0, totalBytes: 0, recentFiles: [], source: "fallback" },
  { id: "deep-learning", name: "深度学习-期末报告", relativePath: "100-学校/深度学习-期末报告", updatedAt: "占位", count: 8, fileCount: 8, markdownCount: 0, totalBytes: 0, recentFiles: [], source: "fallback" },
  { id: "front-dox", name: "front-dox", relativePath: "200-工作/front-dox", updatedAt: "占位", count: 16, fileCount: 16, markdownCount: 0, totalBytes: 0, recentFiles: [], source: "fallback" },
];

const basicNavItems: { id: ViewId; label: string; icon: ReactNode }[] = [
  { id: "dashboard", label: "新对话", icon: <MessageCircle size={17} aria-hidden="true" /> },
  { id: "inbox", label: "收集箱", icon: <Inbox size={17} aria-hidden="true" /> },
  { id: "markdown", label: "Markdown", icon: <FileText size={17} aria-hidden="true" /> },
  { id: "personal", label: "个人页", icon: <UserRound size={17} aria-hidden="true" /> },
  { id: "notes", label: "便利贴", icon: <StickyNote size={17} aria-hidden="true" /> },
];

const importTypes = [
  { label: "TXT / Markdown", icon: <Code2 size={15} aria-hidden="true" /> },
  { label: "PNG / JPG", icon: <FileImage size={15} aria-hidden="true" /> },
  { label: "MP3 / WAV / M4A / AAC", icon: <FileAudio size={15} aria-hidden="true" /> },
  { label: "PDF / DOCX / PPTX 后续", icon: <FileText size={15} aria-hidden="true" /> },
];

function safeJson(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function boundedJson(value: unknown, maxLength = 4000): string {
  const text = safeJson(value);
  return text.length > maxLength ? `${text.slice(0, maxLength)}\n... truncated` : text;
}

function readStorage<T>(key: string, fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    return value ? (JSON.parse(value) as T) : fallback;
  } catch {
    return fallback;
  }
}

function flattenTree(nodes: VaultTreeNode[]): VaultTreeNode[] {
  return nodes.flatMap((node) => [node, ...flattenTree(node.children)]);
}

function parentTreePaths(relativePath: string): string[] {
  const segments = relativePath.split("/").filter(Boolean);
  return segments.slice(0, -1).map((_, index) => segments.slice(0, index + 1).join("/"));
}

function isMarkdownPath(path?: string): path is string {
  return Boolean(path && /\.(md|markdown)$/i.test(path));
}

function directoryPrefix(relativePath: string): string {
  const segments = relativePath.split("/").filter(Boolean);
  return segments.length > 1 ? segments.slice(0, -1).join("/") : "";
}

function formatDate(value?: string | number): string {
  if (!value) return "未记录";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return typeof value === "string" ? value : "未记录";
  return date.toLocaleString("zh-CN", { hour12: false });
}

function dateInputToIso(value: string, edge: "start" | "end"): string | undefined {
  if (!value) return undefined;
  const date = new Date(`${value}T00:00:00`);
  if (Number.isNaN(date.getTime())) return undefined;
  if (edge === "end") {
    date.setHours(23, 59, 59, 999);
  }
  return date.toISOString();
}

function isActiveActionItem(item: TodoScheduleItem): boolean {
  return item.status === "open" || item.status === "scheduled";
}

function isCompletedActionItem(item: TodoScheduleItem): boolean {
  return item.status === "completed";
}

function actionKindLabel(item: TodoScheduleItem | TodoScheduleCandidate): string {
  return item.kind === "schedule" ? "日程" : "TODO";
}

function actionDateLabel(item: TodoScheduleItem): string {
  const value = item.startsAt ?? item.dueAt;
  if (!value) return item.kind === "schedule" ? "未定时间" : "无截止";
  return formatDate(value);
}

function conversationTitle(conversation: RagConversation): string {
  return conversation.title?.trim() || `RAG 对话 #${conversation.id}`;
}

function messageRoleLabel(role: string): string {
  if (role === "user") return "我";
  if (role === "assistant") return "助手";
  if (role === "system") return "系统";
  return role;
}

function formatBudget(cents: number): string {
  return `${(cents / 100).toFixed(2)} 元`;
}

function formatBytes(value?: number): string {
  if (!value) return "0 KB";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function percentage(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function confirmationReasonLabel(reason: string): string {
  const labels: Record<string, string> = {
    target_inside_inbox: "目标仍在收集箱内",
    target_conflict: "目标文件已存在",
    target_not_in_archive_map: "目标目录不在 Archive Map",
    target_missing_archive_directory: "目标缺少归档目录",
    low_confidence: "置信度低",
    classification_boundary_issue: "分类边界不清",
    forced_mock: "mock 计划",
    missing_key: "缺少 MiMo key",
    budget_blocked: "预算阻断",
    parse_error: "AI 响应解析失败",
    fallback: "降级结果",
  };
  return labels[reason] ?? reason;
}

function decisionMetadataFromPayload(payload?: Record<string, unknown>): OrganizeDecisionMetadata | undefined {
  if (!payload) return undefined;
  const confirmationReasons = Array.isArray(payload.confirmationReasons)
    ? payload.confirmationReasons.map((item) => String(item)).filter(Boolean)
    : [];
  if (
    payload.targetArchiveDir === undefined &&
    payload.archiveMapMatched === undefined &&
    payload.proposesNewDirectory === undefined &&
    payload.confirmationRequired === undefined &&
    confirmationReasons.length === 0 &&
    payload.newDirectoryReason === undefined
  ) {
    return undefined;
  }
  return {
    targetArchiveDir: typeof payload.targetArchiveDir === "string" ? payload.targetArchiveDir : undefined,
    archiveMapMatched: Boolean(payload.archiveMapMatched),
    proposesNewDirectory: Boolean(payload.proposesNewDirectory),
    confirmationRequired: Boolean(payload.confirmationRequired),
    confirmationReasons,
    newDirectoryReason: typeof payload.newDirectoryReason === "string" ? payload.newDirectoryReason : undefined,
  };
}

function decisionMetadataSummary(metadata?: OrganizeDecisionMetadata): string | undefined {
  if (!metadata) return undefined;
  const parts = [
    metadata.archiveMapMatched ? "Archive Map 已命中" : "Archive Map 未命中",
    metadata.targetArchiveDir ? `目录 ${metadata.targetArchiveDir}` : undefined,
    metadata.proposesNewDirectory ? "建议新目录" : undefined,
    metadata.confirmationRequired ? "需要确认" : undefined,
  ].filter(Boolean);
  return parts.join(" / ");
}

function hasFallback(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.some((item) => hasFallback(item));
  }
  return Boolean(
    value &&
      typeof value === "object" &&
      "isFallback" in value &&
      (value as { isFallback?: boolean }).isFallback,
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return JSON.stringify(error);
}

function createStickyNote(path: string): StickyNoteRecord {
  const now = new Date().toISOString();
  return {
    id: `note-${Date.now()}`,
    title: "临时记录",
    content: "",
    autosavePath: path,
    pinned: true,
    updatedAt: now,
  };
}

function actionLabel(action: ConflictResolutionAction): string {
  const labels: Record<ConflictResolutionAction, string> = {
    keep_existing: "保留现有",
    overwrite: "覆盖",
    rename: "重命名",
    skip: "跳过",
  };
  return labels[action];
}

function safeConflictActions(conflict: ConflictItem): ConflictResolutionAction[] {
  const fallbackOptions: ConflictResolutionAction[] = ["rename", "skip", "keep_existing"];
  const options = conflict.options?.length ? conflict.options : fallbackOptions;
  return options.filter((action) => action !== "overwrite");
}

function conflictDiffStatusLabel(status?: string): string {
  const labels: Record<string, string> = {
    identical: "文本内容相同",
    missing_file: "文件缺失，无法生成 diff",
    ok: "只读差异预览",
    read_error: "读取失败，无法生成 diff",
    too_large: "文件过大，已跳过 diff",
    unsupported_file_type: "非文本文件，已跳过 diff",
  };
  return status ? labels[status] ?? status : "未生成 diff";
}

function diffLineMarker(kind: ConflictDiffLine["kind"]): string {
  if (kind === "added") return "+";
  if (kind === "removed") return "-";
  if (kind === "omitted") return "...";
  return " ";
}

function diffLineNumberLabel(line: ConflictDiffLine): string {
  if (line.kind === "added") return `T${line.targetLine ?? ""}`;
  if (line.kind === "removed") return `S${line.sourceLine ?? ""}`;
  if (line.kind === "context") return `S${line.sourceLine ?? ""}/T${line.targetLine ?? ""}`;
  return "";
}

function boundedSnippet(value?: string): string {
  if (!value) return "";
  const normalized = value.replace(/\s+/g, " ").trim();
  return normalized.length > 220 ? `${normalized.slice(0, 220)}...` : normalized;
}

function queueStateLabel(state?: QueueStatus["state"]): string {
  const labels: Record<QueueStatus["state"], string> = {
    idle: "空闲",
    running: "运行中",
    paused: "已暂停",
    cooldown: "冷却中",
    error: "异常",
  };
  return state ? labels[state] : "未连接";
}

function archiveMapHealthLabel(status?: string): string {
  const labels: Record<string, string> = {
    ok: "健康",
    stale: "需重建",
    empty: "空地图",
    failed: "失败",
    fallback: "降级",
  };
  return status ? labels[status] ?? status : "未生成";
}

function pageTitle(view: ViewId): { title: string; subtitle: string } {
  const titles: Record<ViewId, { title: string; subtitle: string }> = {
    dashboard: { title: "主仪表盘", subtitle: "本地优先 · 自动整理 · AI 助手" },
    inbox: { title: "收集箱", subtitle: "无脑丢入 · 自动整理 · 本地优先" },
    markdown: { title: "Markdown 编辑器", subtitle: "Obsidian 式 Vault 树 · 编辑预览 · AI 辅助" },
    notes: { title: "便利贴", subtitle: "Ctrl+Space 快速记录 · Ctrl+Enter 保存" },
    personal: { title: "个人页面", subtitle: "学习、目标、复习与知识沉淀" },
    project: { title: "项目工作台", subtitle: "项目资料、Agent 历史与下一步行动" },
    settings: { title: "设置", subtitle: "本地优先 · AI 配置 · 快捷键 · 自动化" },
  };
  return titles[view];
}

function fileIcon(path: string, size = 15): ReactNode {
  if (/\.(png|jpe?g|gif|webp)$/i.test(path)) return <FileImage size={size} aria-hidden="true" />;
  if (/\.(mp3|wav|m4a|flac|aac|ogg)$/i.test(path)) return <FileAudio size={size} aria-hidden="true" />;
  if (/\.md$/i.test(path)) return <Code2 size={size} aria-hidden="true" />;
  if (/\.url$/i.test(path)) return <Link2 size={size} aria-hidden="true" />;
  return <FileText size={size} aria-hidden="true" />;
}

export default function App() {
  const [activeView, setActiveView] = useState<ViewId>("dashboard");
  const [vaultPath, setVaultPath] = useState(() => localStorage.getItem("thebrain.vault") ?? "");
  const [relativePath, setRelativePath] = useState(defaultRelativePath);
  const [content, setContent] = useState("# 新笔记\n");
  const [frontmatterText, setFrontmatterText] = useState(
    '{\n  "title": "新笔记",\n  "status": "draft"\n}',
  );
  const [markdownDoc, setMarkdownDoc] = useState<MarkdownDocument | null>(null);
  const [initResult, setInitResult] = useState<VaultInitResult | null>(null);
  const [tree, setTree] = useState<VaultTreeNode[]>([]);
  const [inbox, setInbox] = useState<InboxItem[]>([]);
  const [ledger, setLedger] = useState<LedgerItem[]>([]);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [ragStatus, setRagStatus] = useState<RagIndexStatus | null>(null);
  const [ragIndexRun, setRagIndexRun] = useState<RagIndexRun | null>(null);
  const [ragAnswer, setRagAnswer] = useState<RagAnswer | null>(null);
  const [ragTrace, setRagTrace] = useState<RagTraceRun | null>(null);
  const [ragIsAsking, setRagIsAsking] = useState(false);
  const [ragConversations, setRagConversations] = useState<RagConversation[]>([]);
  const [activeConversationId, setActiveConversationId] = useState<number | null>(null);
  const [ragMessages, setRagMessages] = useState<RagMessage[]>([]);
  const [ragScopeKind, setRagScopeKind] = useState<RagScopeKind>("allVault");
  const [ragHistoryNotice, setRagHistoryNotice] = useState("");
  const [exported, setExported] = useState("");
  const [status, setStatus] = useState("等待选择 Vault");
  const [appError, setAppError] = useState<AppError | null>(null);
  const [settings, setSettings] = useState<AppSettings>(() =>
    readStorage<AppSettings>(settingsStorageKey, defaultAppSettings),
  );
  const [queueStatus, setQueueStatus] = useState<QueueStatus | null>(null);
  const [listenerStatus, setListenerStatus] = useState<InboxListenerStatus | null>(null);
  const [workerStatus, setWorkerStatus] = useState<WorkerStatus | null>(null);
  const [residentWorkerStatus, setResidentWorkerStatus] = useState<ResidentWorkerStatus | null>(null);
  const [workerRunResult, setWorkerRunResult] = useState<WorkerRunResult | null>(null);
  const [budgetStatus, setBudgetStatus] = useState<BudgetStatus | null>(null);
  const [archiveMap, setArchiveMap] = useState<ArchiveMapSnapshot | null>(null);
  const [archiveRuleDrafts, setArchiveRuleDrafts] = useState<Record<string, ArchiveRuleDraft>>({});
  const [archiveRuleSavingPath, setArchiveRuleSavingPath] = useState("");
  const [conflicts, setConflicts] = useState<ConflictItem[]>([]);
  const [conflictAnswers, setConflictAnswers] = useState<Record<string, string>>({});
  const [conflictActions, setConflictActions] = useState<Record<string, ConflictResolutionAction>>({});
  const [conflictTargets, setConflictTargets] = useState<Record<string, string>>({});
  const [conflictRules, setConflictRules] = useState<ConflictRule[]>([]);
  const [conflictRuleDrafts, setConflictRuleDrafts] = useState<Record<number, Partial<ConflictRule>>>({});
  const [moveLogs, setMoveLogs] = useState<MoveLog[]>([]);
  const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([]);
  const [auditSearchText, setAuditSearchText] = useState("");
  const [auditEventTypeFilter, setAuditEventTypeFilter] = useState("");
  const [auditStartDate, setAuditStartDate] = useState("");
  const [auditEndDate, setAuditEndDate] = useState("");
  const [auditNextBeforeId, setAuditNextBeforeId] = useState<number | undefined>(undefined);
  const [expandedAuditEventId, setExpandedAuditEventId] = useState<number | null>(null);
  const [batchRecoveryPreview, setBatchRecoveryPreview] = useState<BatchRecoveryPreviewState | null>(null);
  const [workspaceInsights, setWorkspaceInsights] = useState<WorkspaceInsights | null>(null);
  const [todoCandidates, setTodoCandidates] = useState<TodoScheduleCandidate[]>([]);
  const [todoScheduleItems, setTodoScheduleItems] = useState<TodoScheduleItem[]>([]);
  const [organizePlan, setOrganizePlan] = useState<AiOrganizePlan | null>(null);
  const [organizeResult, setOrganizeResult] = useState<AiOrganizeResult | null>(null);
  const [inboxPlanResult, setInboxPlanResult] = useState<InboxPlanResult | null>(null);
  const [extractResult, setExtractResult] = useState<MimoExtractResult | null>(null);
  const [importResults, setImportResults] = useState<InboxImportResult[]>([]);
  const [mimoStatus, setMimoStatus] = useState<MimoStatus | null>(null);
  const [mimoKeyInput, setMimoKeyInput] = useState("");
  const [selectedInboxPath, setSelectedInboxPath] = useState("");
  const [targetMovePath, setTargetMovePath] = useState("");
  const [rollbackAuditId, setRollbackAuditId] = useState("");
  const [importBehavior, setImportBehavior] = useState<ImportBehavior>("copy");
  const [dragActive, setDragActive] = useState(false);
  const [chatPrompt, setChatPrompt] = useState("");
  const [projectPrompt, setProjectPrompt] = useState("");
  const [stickyMode, setStickyMode] = useState<"new" | "continue">("new");
  const [stickyNotes, setStickyNotes] = useState<StickyNoteRecord[]>(() =>
    readStorage<StickyNoteRecord[]>(notesStorageKey, [
      createStickyNote(defaultAppSettings.stickyNotesPath),
    ]),
  );
  const [activeNoteId, setActiveNoteId] = useState("");
  const [dirtyNoteId, setDirtyNoteId] = useState<string | null>(null);
  const [selectedProjectId, setSelectedProjectId] = useState("");
  const [expandedTreePaths, setExpandedTreePaths] = useState<Set<string>>(
    () => new Set(parentTreePaths(defaultRelativePath)),
  );

  const allNodes = useMemo(() => flattenTree(tree), [tree]);

  const markdownFiles = useMemo(
    () =>
      allNodes.filter(
        (node) => !node.isDir && /\.(md|markdown)$/i.test(node.relativePath),
      ),
    [allNodes],
  );

  const inboxFiles = useMemo(() => inbox.filter((item) => !item.isDir), [inbox]);
  const vaultFiles = useMemo(() => allNodes.filter((node) => !node.isDir), [allNodes]);

  const projects = useMemo<ProjectItem[]>(() => {
    const insightProjects = workspaceInsights?.isFallback
      ? []
      : (workspaceInsights?.projects ?? []).map((project) => ({
          id: project.id || project.relativePath,
          name: project.name || project.relativePath,
          relativePath: project.relativePath,
          updatedAt: project.updatedAt ?? "未记录",
          count: project.fileCount,
          fileCount: project.fileCount,
          markdownCount: project.markdownCount,
          totalBytes: project.totalBytes,
          recentFiles: project.recentFiles,
          source: "insights" as const,
        }));
    if (insightProjects.length > 0) return insightProjects;

    const topLevelDirs = tree
      .filter((node) => node.isDir && !node.name.startsWith("."))
      .filter((node) => node.name !== "000-收集箱")
      .slice(0, 8)
      .map((node) => {
        const descendants = flattenTree(node.children);
        const files = descendants.filter((child) => !child.isDir);
        const markdownCount = files.filter((child) => isMarkdownPath(child.relativePath)).length;
        return {
          id: node.relativePath,
          name: node.name,
          relativePath: node.relativePath,
          updatedAt: "Vault 树派生",
          count: files.length,
          fileCount: files.length,
          markdownCount,
          totalBytes: 0,
          recentFiles: [],
          source: "tree" as const,
        };
      });
    return topLevelDirs.length > 0 ? topLevelDirs : fallbackProjects;
  }, [tree, workspaceInsights]);

  const selectedProject = useMemo(
    () => projects.find((project) => project.id === selectedProjectId) ?? projects[0],
    [projects, selectedProjectId],
  );

  const activeConversation = useMemo(
    () => ragConversations.find((conversation) => conversation.id === activeConversationId) ?? null,
    [activeConversationId, ragConversations],
  );

  const currentMarkdownRelativePath = useMemo(() => {
    if (isMarkdownPath(markdownDoc?.relativePath)) return markdownDoc.relativePath;
    if (isMarkdownPath(relativePath) && markdownFiles.some((file) => file.relativePath === relativePath)) {
      return relativePath;
    }
    return "";
  }, [markdownDoc?.relativePath, markdownFiles, relativePath]);

  const currentDirectoryPrefix = useMemo(
    () => (currentMarkdownRelativePath ? directoryPrefix(currentMarkdownRelativePath) : ""),
    [currentMarkdownRelativePath],
  );

  const activeNote = useMemo(
    () => stickyNotes.find((note) => note.id === activeNoteId) ?? stickyNotes[0] ?? null,
    [activeNoteId, stickyNotes],
  );

  const pendingCandidates = useMemo(
    () => todoCandidates.filter((candidate) => candidate.status === "pending"),
    [todoCandidates],
  );
  const activeActionItems = useMemo(
    () => todoScheduleItems.filter((item) => !item.isFallback && isActiveActionItem(item)),
    [todoScheduleItems],
  );
  const openTodoItems = useMemo(
    () => activeActionItems.filter((item) => item.kind === "todo"),
    [activeActionItems],
  );
  const scheduledItems = useMemo(
    () => activeActionItems.filter((item) => item.kind === "schedule"),
    [activeActionItems],
  );
  const completedActionItems = useMemo(
    () => todoScheduleItems.filter((item) => !item.isFallback && item.status === "completed"),
    [todoScheduleItems],
  );

  const recentMarkdown = useMemo(() => markdownFiles.slice(0, 5), [markdownFiles]);
  const recoveryQueueItems = useMemo(
    () => [
      ...(queueStatus?.items?.failed ?? []),
      ...(queueStatus?.items?.conflicts ?? []),
    ],
    [queueStatus],
  );
  const rollbackableMoveLogs = useMemo(
    () => moveLogs.filter((log) => log.status === "moved").slice(0, 50),
    [moveLogs],
  );

  const relatedFiles = useMemo(() => {
    if (!relativePath) return recentMarkdown;
    const firstSegment = relativePath.split("/")[0];
    return markdownFiles
      .filter((node) => node.relativePath !== relativePath)
      .filter((node) => node.relativePath.startsWith(firstSegment))
      .slice(0, 4);
  }, [markdownFiles, recentMarkdown, relativePath]);

  const projectRelatedFiles = useMemo<RecentFileSummary[]>(() => {
    if (selectedProject?.recentFiles.length) {
      return selectedProject.recentFiles.slice(0, 5);
    }
    if (!selectedProject?.relativePath) return [];
    return markdownFiles
      .filter((node) => node.relativePath.startsWith(selectedProject.relativePath))
      .slice(0, 5)
      .map((node) => ({
        name: node.name,
        relativePath: node.relativePath,
        extension: "md",
        sizeBytes: 0,
      }));
  }, [markdownFiles, selectedProject]);

  const previewHtml = useMemo(() => {
    const markdown = markdownDoc?.previewMarkdown ?? content;
    const unsafeHtml = marked.parse(markdown, { async: false }) as string;
    return DOMPurify.sanitize(unsafeHtml, { USE_PROFILES: { html: true } });
  }, [content, markdownDoc]);

  const page = pageTitle(activeView);
  const queueIsPaused = !workerStatus?.listener.enabled || workerStatus.listener.status === "paused";
  const listenerIsRunning = Boolean(listenerStatus?.enabled && listenerStatus.status === "running");
  const residentWorkerIsRunning = Boolean(residentWorkerStatus?.running);
  const budgetRatio =
    budgetStatus && budgetStatus.monthlyLimitCents > 0
      ? Math.min(1, budgetStatus.spentCents / budgetStatus.monthlyLimitCents)
      : 0;
  const mimoKeyLabel = mimoStatus?.keyNeedsCleanup
    ? "Key BOM"
    : mimoStatus?.hasKey
      ? "MiMo key"
      : "本地检索";
  const mimoSettingsSummary = mimoStatus?.keyNeedsCleanup
    ? `检测到 BOM：${mimoStatus.keySource ?? "runtime"}`
    : mimoStatus?.hasKey
      ? `已找到 key：${mimoStatus.keySource ?? "runtime"}（未验证连接）`
      : "缺少 MIMO_API_KEY 或本地 .secrets key";
  const insightsAreFallback = hasFallback(workspaceInsights);
  const realWorkspaceInsights = workspaceInsights && !insightsAreFallback ? workspaceInsights : null;
  const insightsFallbackText = insightsAreFallback
    ? `统计降级/占位：${workspaceInsights?.fallbackReason ?? "get_workspace_insights 不可用"}`
    : "";
  const vaultFileCount = realWorkspaceInsights?.vault.fileCount ?? vaultFiles.length;
  const vaultMarkdownCount = realWorkspaceInsights?.vault.markdownCount ?? markdownFiles.length;
  const ragDocumentCount =
    realWorkspaceInsights?.rag.documentCount ?? (ragStatus?.isFallback ? 0 : ragStatus?.documentCount ?? 0);
  const ragConversationCount =
    realWorkspaceInsights?.rag.conversationCount ??
    ragConversations.filter((conversation) => !conversation.isFallback).length;
  const pendingTodoCount =
    realWorkspaceInsights?.candidates.pendingTodo ??
    pendingCandidates.filter((candidate) => candidate.kind === "todo" && !candidate.isFallback).length;
  const pendingScheduleCount =
    realWorkspaceInsights?.candidates.pendingSchedule ??
    pendingCandidates.filter((candidate) => candidate.kind === "schedule" && !candidate.isFallback).length;
  const openTodoCount = realWorkspaceInsights?.actionItems.openTodo ?? openTodoItems.length;
  const scheduledActionCount = realWorkspaceInsights?.actionItems.scheduled ?? scheduledItems.length;
  const completedActionCount = realWorkspaceInsights?.actionItems.completed ?? completedActionItems.length;
  const formalActionCount =
    realWorkspaceInsights?.actionItems.total ?? todoScheduleItems.filter((item) => !item.isFallback).length;
  const stickyActiveCount =
    realWorkspaceInsights?.sticky.active ?? stickyNotes.filter((note) => !note.isFallback).length;
  const movementCompletedCount =
    realWorkspaceInsights?.movement.completed ??
    moveLogs.filter((log) => ["completed", "moved"].includes(log.status)).length;
  const movementIssueCount =
    realWorkspaceInsights
      ? realWorkspaceInsights.movement.conflict + realWorkspaceInsights.movement.failed
      : moveLogs.filter((log) => ["conflict", "failed"].includes(log.status)).length;
  const recentAuditCount =
    realWorkspaceInsights?.audit.recentCount ?? auditEvents.filter((event) => !event.isFallback).length;
  const latestAuditEventType = realWorkspaceInsights?.audit.latestEventType ?? auditEvents[0]?.eventType;
  const latestAuditAt = realWorkspaceInsights?.audit.latestAt ?? auditEvents[0]?.createdAt;
  const recentWorkspaceFiles =
    realWorkspaceInsights?.recentFiles.length
      ? realWorkspaceInsights.recentFiles.slice(0, 5)
      : recentMarkdown.map((file) => ({
          name: file.name,
          relativePath: file.relativePath,
          extension: "md",
          sizeBytes: 0,
          modifiedAt: undefined,
        }));
  const auditEventTypes = useMemo(
    () => Array.from(new Set(auditEvents.map((event) => event.eventType).filter(Boolean))).sort(),
    [auditEvents],
  );

  useEffect(() => {
    localStorage.setItem(settingsStorageKey, JSON.stringify(settings));
  }, [settings]);

  useEffect(() => {
    localStorage.setItem(notesStorageKey, JSON.stringify(stickyNotes));
  }, [stickyNotes]);

  useEffect(() => {
    if (!activeNoteId && stickyNotes[0]) {
      setActiveNoteId(stickyNotes[0].id);
    }
  }, [activeNoteId, stickyNotes]);

  useEffect(() => {
    if (projects[0] && (!selectedProjectId || !projects.some((project) => project.id === selectedProjectId))) {
      setSelectedProjectId(projects[0].id);
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    if (ragScopeKind === "currentFile" && !currentMarkdownRelativePath) {
      setRagScopeKind("allVault");
    }
    if (ragScopeKind === "projectPrefix" && !currentDirectoryPrefix && !selectedProject?.relativePath) {
      setRagScopeKind("allVault");
    }
  }, [currentDirectoryPrefix, currentMarkdownRelativePath, ragScopeKind, selectedProject?.relativePath]);

  useEffect(() => {
    const parentPaths = parentTreePaths(relativePath);
    if (parentPaths.length === 0) return;
    setExpandedTreePaths((current) => {
      let changed = false;
      const next = new Set(current);
      for (const path of parentPaths) {
        if (!next.has(path)) {
          next.add(path);
          changed = true;
        }
      }
      return changed ? next : current;
    });
  }, [relativePath]);

  useEffect(() => {
    if (!dirtyNoteId) return;
    const dirtyNote = stickyNotes.find((note) => note.id === dirtyNoteId);
    if (!dirtyNote) return;
    const delay = Math.max(800, settings.autoSaveIntervalSeconds * 1000);
    const timeoutId = window.setTimeout(() => {
      void persistStickyNote(dirtyNote, true);
    }, delay);
    return () => window.clearTimeout(timeoutId);
  }, [dirtyNoteId, settings.autoSaveIntervalSeconds, stickyNotes, vaultPath]);

  useEffect(() => {
    if (vaultPath) {
      void refresh(vaultPath);
    } else {
      void refreshOperations("");
    }
  }, []);

  async function run<T>(
    action: () => Promise<T>,
    message: string,
    successMessage = "完成",
  ): Promise<T | null> {
    setAppError(null);
    setStatus(message);
    try {
      const result = await action();
      setStatus(hasFallback(result) ? `${successMessage}（前端占位）` : successMessage);
      return result;
    } catch (error) {
      setStatus("失败");
      setAppError({
        title: `${message}失败`,
        detail: errorMessage(error),
        recovery: "检查 Vault 路径、文件占用状态或后端命令注册；前端不会自动覆盖本地文件。",
      });
      return null;
    }
  }

  async function refreshOperations(nextVaultPath = vaultPath) {
    const [nextSettings, nextQueue, nextListener, nextWorker, nextResidentWorker, nextBudget, nextArchiveMap, nextConflicts, nextConflictRules, nextMoveLogs, nextAuditEvents, nextWorkspaceInsights, nextTodoCandidates, nextTodoScheduleItems, nextNotes, nextMimoStatus, nextRagStatus, nextRagConversations] =
      await Promise.all([
        commands.getAppSettings(nextVaultPath),
        commands.getQueueStatus(nextVaultPath),
        nextVaultPath ? commands.getInboxListenerStatus(nextVaultPath) : Promise.resolve(null),
        nextVaultPath ? commands.getWorkerStatus(nextVaultPath) : Promise.resolve(null),
        nextVaultPath ? commands.getResidentWorkerStatus(nextVaultPath) : Promise.resolve(null),
        commands.getBudgetStatus(nextVaultPath),
        nextVaultPath ? commands.getArchiveMap(nextVaultPath) : Promise.resolve(null),
        commands.listConflicts(nextVaultPath),
        nextVaultPath ? commands.listConflictRules(nextVaultPath, true) : Promise.resolve([]),
        nextVaultPath ? commands.listMoveLogs(nextVaultPath) : Promise.resolve([]),
        nextVaultPath ? commands.listAuditEvents(nextVaultPath, 30) : Promise.resolve([]),
        nextVaultPath ? commands.getWorkspaceInsights(nextVaultPath) : Promise.resolve(null),
        nextVaultPath ? commands.listTodoScheduleCandidates(nextVaultPath) : Promise.resolve([]),
        nextVaultPath ? commands.listTodoScheduleItems(nextVaultPath, true) : Promise.resolve([]),
        commands.listStickyNotes(nextVaultPath),
        nextVaultPath ? commands.getMimoStatus(nextVaultPath) : Promise.resolve(null),
        nextVaultPath ? commands.getRagIndexStatus(nextVaultPath) : Promise.resolve(null),
        nextVaultPath ? commands.listRagConversations(nextVaultPath, 20) : Promise.resolve([]),
      ]);

    setSettings((current) =>
      nextSettings.isFallback ? { ...current, isFallback: true } : { ...current, ...nextSettings },
    );
    setQueueStatus(nextQueue);
    setListenerStatus(nextListener);
    setWorkerStatus(nextWorker);
    setResidentWorkerStatus(nextResidentWorker);
    setBudgetStatus(nextBudget);
    setArchiveMap(nextArchiveMap);
    setConflicts(nextConflicts);
    setConflictRules(nextConflictRules);
    setMoveLogs(nextMoveLogs);
    setAuditEvents(nextAuditEvents);
    setAuditNextBeforeId(nextAuditEvents.length >= 30 ? nextAuditEvents.at(-1)?.id : undefined);
    setExpandedAuditEventId(null);
    setWorkspaceInsights(nextWorkspaceInsights);
    setTodoCandidates(nextTodoCandidates);
    setTodoScheduleItems(nextTodoScheduleItems);
    if (nextNotes.length > 0) {
      setStickyNotes(nextNotes);
    }
    setMimoStatus(nextMimoStatus);
    setRagStatus(nextRagStatus);
    setRagConversations(nextRagConversations);
    if (activeConversationId && !nextRagConversations.some((conversation) => conversation.id === activeConversationId)) {
      setActiveConversationId(null);
      setRagMessages([]);
    }
  }

  async function chooseVault() {
    const selected = await run(() => selectVault(), "选择 Vault", "Vault 已选择");
    if (selected) {
      setVaultPath(selected);
      localStorage.setItem("thebrain.vault", selected);
      await refresh(selected);
    }
  }

  async function refresh(nextVaultPath = vaultPath) {
    if (!nextVaultPath) {
      await refreshOperations("");
      return;
    }

    const result = await run(
      async () => {
        const [nextTree, nextInbox, nextLedger, nextUsage] = await Promise.all([
          commands.listVaultTree(nextVaultPath),
          commands.listInbox(nextVaultPath),
          commands.parseInboxLedger(nextVaultPath),
          commands.getAiUsage(nextVaultPath),
        ]);
        return { nextTree, nextInbox, nextLedger, nextUsage };
      },
      "刷新 Vault",
      "Vault 已刷新",
    );
    if (!result) return;
    setTree(result.nextTree);
    setInbox(result.nextInbox);
    setLedger(result.nextLedger);
    setUsage(result.nextUsage);
    await refreshOperations(nextVaultPath);
  }

  async function initVault() {
    if (!vaultPath) return;
    const result = await run(() => commands.initVault(vaultPath), "初始化 Vault", "Vault 已初始化");
    if (!result) return;
    setInitResult(result);
    await refresh(result.vaultPath);
  }

  async function openMarkdown(path: string) {
    if (!vaultPath || !path) return;
    const result = await run(() => commands.readMarkdown(vaultPath, path), "读取 Markdown", "已打开");
    if (!result) return;
    setMarkdownDoc(result);
    setRelativePath(result.relativePath);
    setContent(result.content);
    setFrontmatterText(safeJson(result.frontmatter));
    setExported("");
    setActiveView("markdown");
  }

  function toggleTreeFolder(path: string) {
    setExpandedTreePaths((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  async function saveMarkdown() {
    if (!vaultPath || !relativePath) return;
    let frontmatter: unknown;
    try {
      frontmatter = JSON.parse(frontmatterText || "{}");
    } catch {
      setAppError({
        title: "YAML 属性无效",
        detail: "当前属性编辑器内容不是合法 JSON。",
        recovery: "修正 JSON 后再保存，Markdown 正文暂未提交。",
      });
      return;
    }
    const result = await run(
      () => commands.saveMarkdown(vaultPath, relativePath, content, frontmatter),
      "保存 Markdown",
      "Markdown 已保存",
    );
    if (!result) return;
    setMarkdownDoc(result);
    setFrontmatterText(safeJson(result.frontmatter));
    await refresh();
  }

  async function exportMarkdown() {
    if (!vaultPath || !relativePath) return;
    const result = await run(
      () => commands.exportMarkdown(vaultPath, relativePath),
      "导出 Markdown",
      "Markdown 已导出",
    );
    if (!result) return;
    setExported(result.markdown);
  }

  async function rebuildRagIndex() {
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "RAG 索引需要先绑定一个本地 Vault。",
        recovery: "选择并初始化 Vault 后再重建索引。",
      });
      return;
    }
    const result = await run(
      () => commands.rebuildRagIndex(vaultPath),
      "重建 RAG 索引",
      "RAG 索引已更新",
    );
    if (!result) return;
    setRagIndexRun(result);
    const nextStatus = await commands.getRagIndexStatus(vaultPath);
    setRagStatus(nextStatus);
  }

  async function saveSettings() {
    const result = await run(
      async () => {
        const saved = await commands.saveAppSettings(vaultPath, settings);
        if (settings.enableGlobalShortcut) {
          await commands.registerGlobalShortcut("Ctrl+Space");
        }
        await commands.prewarmStickyWindows(settings.prewarmWindows);
        return saved;
      },
      "保存设置",
      "设置已保存",
    );
    if (!result) return;
    setSettings((current) => ({ ...current, ...result }));
  }

  async function saveBudget() {
    const result = await run(
      () =>
        commands.saveBudgetSettings(vaultPath, {
          monthlyLimitCents: settings.budgetMonthlyCents,
          hardStopCents: settings.budgetHardStopCents,
        }),
      "保存预算",
      "预算已保存",
    );
    if (!result) return;
    setBudgetStatus(result);
  }

  async function saveMimoKey() {
    const apiKey = mimoKeyInput.trim();
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "MiMo key 会保存到当前 Vault 的 .secrets 目录。",
        recovery: "先选择并初始化 Vault，再保存 API key。",
      });
      return;
    }
    if (!apiKey) {
      setAppError({
        title: "API Key 为空",
        detail: "请输入 MiMo API key 后再保存。",
        recovery: "从 MiMo 控制台复制 key，粘贴到设置页输入框。",
      });
      return;
    }
    const result = await run(
      () => commands.saveMimoApiKey(vaultPath, apiKey),
      "保存 MiMo API Key",
      "MiMo API Key 已保存",
    );
    if (!result) return;
    setMimoStatus(result);
    setMimoKeyInput("");
  }

  async function toggleInboxListener() {
    if (!vaultPath) return;
    const result = await run(
      () => (listenerIsRunning ? commands.stopInboxListener(vaultPath) : commands.startInboxListener(vaultPath)),
      listenerIsRunning ? "停止收集箱监听" : "启动收集箱监听",
      listenerIsRunning ? "收集箱监听已停止" : "收集箱监听已启动",
    );
    if (!result) return;
    setQueueStatus(result);
    const nextListener = await commands.getInboxListenerStatus(vaultPath);
    setListenerStatus(nextListener);
  }

  async function runWorkerControl() {
    if (!vaultPath) return;
    if (queueIsPaused) {
      const resumed = await run(
        () => commands.resumeInboxWorker(vaultPath),
        "恢复后台 worker",
        "后台 worker 已恢复",
      );
      if (!resumed) return;
      setWorkerStatus(resumed);
    }
    await runInboxWorker();
  }

  async function toggleResidentWorker() {
    if (!vaultPath) return;
    const nextStatus = await run(
      () =>
        residentWorkerIsRunning
          ? commands.stopResidentWorker(vaultPath)
          : commands.startResidentWorker(vaultPath),
      residentWorkerIsRunning ? "停止常驻 worker" : "启动常驻 worker",
      residentWorkerIsRunning ? "常驻 worker 正在停止" : "常驻 worker 已启动",
    );
    if (!nextStatus) return;
    setResidentWorkerStatus(nextStatus);
    await refreshOperations(vaultPath);
  }

  async function runInboxWorker() {
    if (!vaultPath) return;
    const result = await run(
      () => commands.runInboxWorker(vaultPath, 8),
      "运行后台 worker",
      "后台 worker 已完成一轮消费",
    );
    if (!result) return;
    setWorkerRunResult(result);
    await refresh(vaultPath);
  }

  async function scanInboxQueue() {
    if (!vaultPath) return;
    const result = await run(
      () => commands.scanInboxQueue(vaultPath),
      "扫描收集箱入队",
      "扫描已完成",
    );
    if (!result) return;
    setQueueStatus(result);
    await refreshOperations(vaultPath);
  }

  async function retryQueueItem(item: QueueItem) {
    if (!vaultPath) return;
    const result = await run(
      () => commands.retryQueueItem(vaultPath, item.id),
      "重试队列项",
      "队列项已恢复待处理",
    );
    if (!result) return;
    await refreshOperations(vaultPath);
  }

  async function skipQueueItem(item: QueueItem) {
    if (!vaultPath) return;
    const result = await run(
      () => commands.skipQueueItem(vaultPath, item.id, "user skipped from inbox recovery panel"),
      "跳过队列项",
      "队列项已跳过",
    );
    if (!result) return;
    await refreshOperations(vaultPath);
  }

  async function previewQueueRecovery(action: "retry" | "skip") {
    if (!vaultPath) return;
    const ids = recoveryQueueItems.slice(0, 50).map((item) => item.id);
    if (ids.length === 0) return;
    const result = await run(
      () => commands.previewQueueRecovery(vaultPath, action, ids),
      action === "retry" ? "预览批量重试" : "预览批量跳过",
      "批量恢复预览已生成",
    );
    if (!result) return;
    setBatchRecoveryPreview({
      kind: action === "retry" ? "queue-retry" : "queue-skip",
      preview: result,
    });
  }

  async function confirmQueueRecoveryPreview(preview: BatchQueueRecoveryPreview) {
    if (!vaultPath) return;
    const ids = preview.items.filter((item) => item.eligible).map((item) => item.id);
    if (ids.length === 0) return;
    const result = await run(
      () =>
        preview.action === "skip"
          ? commands.skipQueueItems(vaultPath, ids, "batch skipped from inbox status panel")
          : commands.retryQueueItems(vaultPath, ids),
      preview.action === "skip" ? "批量跳过队列项" : "批量重试队列项",
      preview.action === "skip" ? "批量跳过已完成" : "批量重试已完成",
    );
    if (!result) return;
    setStatus(
      `${preview.action === "skip" ? "批量跳过" : "批量重试"}完成：成功 ${result.succeeded.length}，失败 ${result.failed.length}`,
    );
    setBatchRecoveryPreview(null);
    await refreshOperations(vaultPath);
  }

  async function rollbackMoveLog(log: MoveLog) {
    if (!vaultPath) return;
    const result = await run(
      () => commands.rollbackMove(vaultPath, String(log.id), log.sourceRelativePath),
      "回滚移动记录",
      "移动记录已回滚",
    );
    if (!result) return;
    await refresh(vaultPath);
  }

  async function previewVisibleMoveLogRollback() {
    if (!vaultPath) return;
    const ids = rollbackableMoveLogs.map((log) => log.id);
    if (ids.length === 0) return;
    const result = await run(
      () => commands.previewRollbackMoves(vaultPath, ids),
      "预览批量回滚",
      "批量回滚预览已生成",
    );
    if (!result) return;
    setBatchRecoveryPreview({ kind: "rollback", preview: result });
  }

  async function confirmRollbackPreview(preview: BatchRollbackPreview) {
    if (!vaultPath) return;
    const ids = preview.items.filter((item) => item.eligible).map((item) => item.id);
    if (ids.length === 0) return;
    const result = await run(
      () => commands.rollbackMoves(vaultPath, ids),
      "批量回滚移动记录",
      "批量回滚已完成",
    );
    if (!result) return;
    setStatus(`批量回滚完成：成功 ${result.rolledBack.length}，失败 ${result.failed.length}`);
    setBatchRecoveryPreview(null);
    await refresh(vaultPath);
  }

  async function importFiles(sourcePaths?: string[]) {
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "导入前需要先选择并初始化 Vault。",
        recovery: "点击左上角 Vault 按钮选择本地 Vault，再执行导入。",
      });
      return;
    }
    const paths = sourcePaths ?? (await selectImportFiles());
    if (paths.length === 0) return;
    const result = await run(
      () => commands.importToInbox(vaultPath, paths, importBehavior),
      "导入文件到收集箱",
      "导入已完成",
    );
    if (!result) return;
    setImportResults(result);
    const firstImported = result.find((item) =>
      ["imported", "already_inbox", "conflict"].includes(item.status),
    );
    if (firstImported?.relativePath) {
      setSelectedInboxPath(firstImported.relativePath);
      if (firstImported.status !== "conflict") {
        setTargetMovePath(firstImported.relativePath.replace(/^000-[^/]+\//, "100-Organized/"));
      }
    }
    await refresh(vaultPath);
  }

  async function planOrganize() {
    if (!vaultPath) return;
    const source = selectedInboxPath || inboxFiles[0]?.relativePath;
    if (!source) {
      setAppError({
        title: "没有可整理的收集箱文件",
        detail: "请先导入或选择 000-收集箱 内的文件。",
        recovery: "v0.3 只对收集箱内的 md/txt、音频和图片生成整理计划。",
      });
      return;
    }
    const result = await run(
      () => commands.planInboxItem(vaultPath, source),
      "生成整理计划",
      "整理计划已生成",
    );
    if (!result) return;
    setInboxPlanResult(result);
    setExtractResult(result.extraction);
    setSelectedInboxPath(result.plan.sourceRelativePath);
    setTargetMovePath(result.plan.targetRelativePath);
    setBudgetStatus(result.budget);
    setOrganizePlan({
      id: `${result.plan.provider}-${Date.now()}`,
      summary: result.plan.summary || result.plan.reason || "整理计划已生成",
      candidates: [
        {
          id: result.plan.sourceRelativePath,
          sourceRelativePath: result.plan.sourceRelativePath,
          targetRelativePath: result.plan.targetRelativePath,
          targetArchiveDir: result.plan.targetArchiveDir,
          archiveMapMatched: result.plan.archiveMapMatched,
          proposesNewDirectory: result.plan.proposesNewDirectory,
          confirmationRequired: result.plan.confirmationRequired,
          confirmationReasons: result.plan.confirmationReasons,
          newDirectoryReason: result.plan.newDirectoryReason,
          confidence: result.plan.confidence,
          reason: result.plan.reason || result.plan.error || "",
          tags: result.plan.tags,
          isFallback: result.plan.isMock || result.extraction.isMock,
          fallbackReason: result.plan.error || result.extraction.error,
        },
      ],
      createdAt: new Date().toISOString(),
      isFallback: result.plan.isMock || result.extraction.isMock,
      fallbackReason: result.plan.error || result.extraction.error,
    });
    setActiveView("inbox");
    await refreshOperations(vaultPath);
  }

  async function rebuildArchiveMap() {
    if (!vaultPath) return;
    const result = await run(
      () => commands.rebuildArchiveMap(vaultPath),
      "重建 Archive Map",
      "Archive Map 已重建",
    );
    if (!result) return;
    setArchiveMap(result);
    await refreshOperations(vaultPath);
  }

  function archiveRuleDraft(directory: ArchiveMapDirectory): ArchiveRuleDraft {
    return (
      archiveRuleDrafts[directory.relativePath] ?? {
        userNote: directory.rule?.userNote ?? "",
        organizingHint: directory.rule?.organizingHint ?? "",
        locked: directory.rule?.locked ?? false,
      }
    );
  }

  function updateArchiveRuleDraft(directory: ArchiveMapDirectory, patch: Partial<ArchiveRuleDraft>) {
    setArchiveRuleDrafts((current) => ({
      ...current,
      [directory.relativePath]: {
        userNote: current[directory.relativePath]?.userNote ?? directory.rule?.userNote ?? "",
        organizingHint:
          current[directory.relativePath]?.organizingHint ?? directory.rule?.organizingHint ?? "",
        locked: current[directory.relativePath]?.locked ?? directory.rule?.locked ?? false,
        ...patch,
      },
    }));
  }

  async function saveArchiveRule(directory: ArchiveMapDirectory) {
    if (!vaultPath) return;
    const draft = archiveRuleDraft(directory);
    setArchiveRuleSavingPath(directory.relativePath);
    try {
      const result = await run(
        () =>
          commands.saveArchiveMapDirectoryRule(vaultPath, {
            relativePath: directory.relativePath,
            userNote: draft.userNote,
            organizingHint: draft.organizingHint,
            locked: draft.locked,
            status: "active",
          }),
        "保存 Archive Map 目录规则",
        "目录规则已保存",
      );
      if (!result) return;
      setArchiveMap(result.snapshot);
      setArchiveRuleDrafts((current) => ({
        ...current,
        [directory.relativePath]: {
          userNote: result.rule.userNote,
          organizingHint: result.rule.organizingHint,
          locked: result.rule.locked,
        },
      }));
      await refreshOperations(vaultPath);
    } finally {
      setArchiveRuleSavingPath("");
    }
  }

  async function runOrganize() {
    const candidate = organizePlan?.candidates[0];
    const source = selectedInboxPath || candidate?.sourceRelativePath;
    const target = targetMovePath || candidate?.targetRelativePath;
    if (!source || !target) {
      setAppError({
        title: "整理移动信息不完整",
        detail: "执行前需要明确 sourceRelativePath 和 targetRelativePath。",
        recovery: "先对一个收集箱文件生成整理计划，或手动填写移动目标。",
      });
      return;
    }
    const result = await run(
      () =>
        commands.runAiOrganize(
          vaultPath,
          organizePlan?.id,
          source,
          target,
          candidate?.reason || inboxPlanResult?.plan.reason,
        ),
      "执行 AI 整理",
      "AI 整理已执行",
    );
    if (!result) return;
    setOrganizeResult(result);
    if (result.auditId) setRollbackAuditId(result.auditId);
    await refresh(vaultPath);
  }

  async function moveInboxItem() {
    if (!selectedInboxPath || !targetMovePath) {
      setAppError({
        title: "移动信息不完整",
        detail: "需要选择收集箱文件并填写目标相对路径。",
        recovery: "目标路径必须仍位于 Vault 内，后端会负责最终校验。",
      });
      return;
    }
    const result = await run(
      () => commands.moveInboxItem(vaultPath, selectedInboxPath, targetMovePath, false),
      "移动收集箱文件",
      "移动请求已提交",
    );
    if (!result) return;
    setOrganizeResult({
      moved: result.moved ? 1 : 0,
      skipped: result.moved ? 0 : 1,
      conflicts: 0,
      auditId: result.auditId,
      message: result.message,
      isFallback: result.isFallback,
      fallbackReason: result.fallbackReason,
    });
    if (result.auditId) setRollbackAuditId(result.auditId);
    await refresh(vaultPath);
  }

  async function rollbackMove() {
    const result = await run(
      () => commands.rollbackMove(vaultPath, rollbackAuditId || undefined, selectedInboxPath || undefined),
      "回滚移动",
      "回滚请求已提交",
    );
    if (!result) return;
    setOrganizeResult({
      moved: 0,
      skipped: result.rolledBack ? 0 : 1,
      conflicts: 0,
      auditId: result.auditId,
      message: result.message,
      isFallback: result.isFallback,
      fallbackReason: result.fallbackReason,
    });
    await refresh(vaultPath);
  }

  async function applyAuditSearch() {
    if (!vaultPath) return;
    const result = await run(
      () => commands.searchAuditEvents(vaultPath, buildAuditSearchQuery()),
      "搜索 audit timeline",
      "audit timeline 已筛选",
    );
    if (!result) return;
    setAuditEvents(result.events);
    setAuditNextBeforeId(result.nextBeforeId);
    setExpandedAuditEventId(null);
    setStatus(`audit 筛选完成：${result.events.length} / limit ${result.appliedLimit}`);
  }

  function buildAuditSearchQuery(beforeId?: number): AuditSearchQuery {
    return {
      text: auditSearchText.trim() || undefined,
      eventTypes: auditEventTypeFilter ? [auditEventTypeFilter] : undefined,
      createdAfter: dateInputToIso(auditStartDate, "start"),
      createdBefore: dateInputToIso(auditEndDate, "end"),
      limit: 30,
      beforeId,
    };
  }

  async function loadMoreAuditEvents() {
    if (!vaultPath || !auditNextBeforeId) return;
    const result = await run(
      () => commands.searchAuditEvents(vaultPath, buildAuditSearchQuery(auditNextBeforeId)),
      "加载更多 audit timeline",
      "audit timeline 已加载更多",
    );
    if (!result) return;
    setAuditEvents((events) => {
      const seen = new Set(events.map((event) => event.id));
      return [...events, ...result.events.filter((event) => !seen.has(event.id))];
    });
    setAuditNextBeforeId(result.events.length > 0 ? result.nextBeforeId : undefined);
    setStatus(`audit 已加载更多：新增 ${result.events.length} 条`);
  }

  async function resetAuditSearch() {
    if (!vaultPath) return;
    setAuditSearchText("");
    setAuditEventTypeFilter("");
    setAuditStartDate("");
    setAuditEndDate("");
    setExpandedAuditEventId(null);
    const events = await run(
      () => commands.listAuditEvents(vaultPath, 30),
      "重置 audit timeline",
      "audit timeline 已重置",
    );
    if (!events) return;
    setAuditEvents(events);
    setAuditNextBeforeId(events.length >= 30 ? events.at(-1)?.id : undefined);
  }

  async function confirmCandidate(candidate: TodoScheduleCandidate) {
    const result = await run(
      () => commands.promoteTodoScheduleCandidate(vaultPath, candidate.id),
      "确认候选",
      candidate.kind === "schedule" ? "已加入正式日程" : "已创建正式 TODO",
    );
    if (!result) return;
    setTodoCandidates((items) =>
      items.map((item) => (item.id === candidate.id ? { ...item, ...result.candidate } : item)),
    );
    if (result.item) {
      setTodoScheduleItems((items) => {
        const withoutCurrent = items.filter(
          (item) => !(item.kind === result.item?.kind && item.id === result.item.id),
        );
        return [result.item as TodoScheduleItem, ...withoutCurrent];
      });
    }
    await refreshOperations(vaultPath);
  }

  async function dismissCandidate(candidate: TodoScheduleCandidate) {
    const result = await run(
      () => commands.dismissTodoScheduleCandidate(vaultPath, candidate.id),
      "忽略候选",
      "候选已忽略",
    );
    if (!result) return;
    setTodoCandidates((items) =>
      items.map((item) => (item.id === candidate.id ? { ...item, status: "dismissed" } : item)),
    );
    await refreshOperations(vaultPath);
  }

  async function updateActionItemStatus(item: TodoScheduleItem, nextStatus: string) {
    const result = await run(
      () =>
        item.kind === "schedule"
          ? commands.setScheduleItemStatus(vaultPath, item.id, nextStatus)
          : commands.setTodoItemStatus(vaultPath, item.id, nextStatus),
      "更新行动项",
      nextStatus === "completed" ? "行动项已完成" : "行动项已更新",
    );
    if (!result) return;
    setTodoScheduleItems((items) =>
      items.map((current) =>
        current.kind === result.kind && current.id === result.id ? { ...current, ...result } : current,
      ),
    );
    await refreshOperations(vaultPath);
  }

  async function completeActionItem(item: TodoScheduleItem) {
    await updateActionItemStatus(item, "completed");
  }

  async function cancelActionItem(item: TodoScheduleItem) {
    await updateActionItemStatus(item, "cancelled");
  }

  function addStickyNote() {
    const note = createStickyNote(settings.stickyNotesPath);
    setStickyNotes((notes) => [note, ...notes]);
    setActiveNoteId(note.id);
    setStickyMode("new");
    setDirtyNoteId(note.id);
  }

  function updateActiveNote(patch: Partial<StickyNoteRecord>) {
    if (!activeNote) return;
    const updatedAt = new Date().toISOString();
    setStickyNotes((notes) =>
      notes.map((note) =>
        note.id === activeNote.id ? { ...note, ...patch, updatedAt } : note,
      ),
    );
    setDirtyNoteId(activeNote.id);
  }

  async function persistStickyNote(note: StickyNoteRecord, silent = false) {
    try {
      const result = vaultPath
        ? await commands.saveStickyNote(vaultPath, note)
        : {
            ...note,
            updatedAt: new Date().toISOString(),
            isFallback: true,
            fallbackReason: "未选择 Vault，已写入浏览器本地缓存。",
          };
      setStickyNotes((notes) => notes.map((item) => (item.id === note.id ? result : item)));
      setDirtyNoteId((current) => (current === note.id ? null : current));
      setStatus(`${silent ? "便利贴已自动保存" : "便利贴已保存"}：${result.autosavePath}`);
    } catch (error) {
      setAppError({
        title: "便利贴保存失败",
        detail: errorMessage(error),
        recovery: "内容已保留在当前界面和浏览器本地缓存，可稍后重试。",
      });
    }
  }

  async function deleteActiveNote() {
    if (!activeNote) return;
    const result = await run(
      () => commands.deleteStickyNote(vaultPath, activeNote.id),
      "删除便利贴",
      "便利贴已删除",
    );
    if (!result) return;
    setStickyNotes((notes) => notes.filter((note) => note.id !== activeNote.id));
    setDirtyNoteId((current) => (current === activeNote.id ? null : current));
  }

  function selectedConflictAction(conflict: ConflictItem): ConflictResolutionAction {
    const actions = safeConflictActions(conflict);
    const stored = conflictActions[conflict.id];
    if (stored && actions.includes(stored)) return stored;
    if (conflict.recommendedAction !== "overwrite" && actions.includes(conflict.recommendedAction)) {
      return conflict.recommendedAction;
    }
    return actions.includes("rename") ? "rename" : actions[0] ?? "skip";
  }

  function selectedConflictTarget(conflict: ConflictItem): string {
    return conflictTargets[conflict.id] ?? conflict.renameSuggestions?.[0]?.targetRelativePath ?? conflict.targetRelativePath ?? "";
  }

  function conflictRuleDraft(rule: ConflictRule): ConflictRule {
    return {
      ...rule,
      ...conflictRuleDrafts[rule.id],
    };
  }

  function updateConflictRuleDraft(ruleId: number, patch: Partial<ConflictRule>) {
    setConflictRuleDrafts((drafts) => ({
      ...drafts,
      [ruleId]: {
        ...drafts[ruleId],
        ...patch,
      },
    }));
  }

  function renderConflictDetails(conflict: ConflictItem) {
    const recommendation = conflict.recommendations?.[0];
    const rule = recommendation?.rule;
    const preview = conflict.preview ?? conflict.context;
    const sourceExists = preview?.sourceExists ?? preview?.source?.exists;
    const targetExists = preview?.targetExists ?? preview?.target?.exists;
    const sourceSize = preview?.sourceSizeBytes ?? preview?.source?.sizeBytes;
    const targetSize = preview?.targetSizeBytes ?? preview?.target?.sizeBytes;
    const sourceSnippet = boundedSnippet(preview?.sourceSnippet ?? preview?.source?.snippet);
    const targetSnippet = boundedSnippet(preview?.targetSnippet ?? preview?.target?.snippet);
    const context = boundedSnippet(preview?.context);
    const diff = preview?.diff;
    const decisionMetadata = decisionMetadataFromPayload(conflict.payload);
    const metadataSummary = decisionMetadataSummary(decisionMetadata);
    const reasonLabels = decisionMetadata?.confirmationReasons.map(confirmationReasonLabel) ?? [];
    return (
      <>
        {rule ? (
          <div className="conflict-recommendation-detail">
            <small>Recommendation: {actionLabel(rule.action)} / {rule.answer || "no answer"} / {Math.round((recommendation?.score ?? 0) * 100)}%</small>
            <small>{recommendation?.reason || rule.matchSummary || "No match reason"} / hits {recommendation?.hitCount ?? rule.hitCount ?? 0} / {recommendation?.autoApply ?? rule.autoApply ? "autoApply on" : "autoApply off"} / {recommendation?.status ?? rule.status ?? "open"}</small>
          </div>
        ) : (
          <small>No matching rule yet. Record one after choosing an action.</small>
        )}
        {decisionMetadata ? (
          <div className="conflict-metadata-detail">
            {metadataSummary ? <small>{metadataSummary}</small> : null}
            {reasonLabels.length > 0 ? <small>确认原因：{reasonLabels.join(" / ")}</small> : null}
            {decisionMetadata.newDirectoryReason ? <small>新目录理由：{decisionMetadata.newDirectoryReason}</small> : null}
          </div>
        ) : null}
        {preview ? (
          <div className="conflict-preview">
            <small>Preview: source {sourceExists === undefined ? "unknown" : sourceExists ? "exists" : "missing"} / {formatBytes(sourceSize)}; target {targetExists === undefined ? "unknown" : targetExists ? "exists" : "missing"} / {formatBytes(targetSize)}</small>
            {sourceSnippet ? <code>Source: {sourceSnippet}</code> : null}
            {targetSnippet ? <code>Target: {targetSnippet}</code> : null}
            {context ? <code>Context: {context}</code> : null}
            {diff ? (
              <div className="conflict-diff-preview" aria-label="Conflict read-only diff preview">
                <small>
                  Diff: {conflictDiffStatusLabel(diff.status)} / source {diff.sourceLineCount} lines / target {diff.targetLineCount} lines{diff.truncated ? " / truncated" : ""}
                </small>
                {diff.status === "ok" && diff.lines.length > 0 ? (
                  <div className="conflict-diff-lines">
                    {diff.lines.map((line, index) => (
                      <code className={`diff-line diff-${line.kind}`} key={`${line.kind}-${line.sourceLine ?? "x"}-${line.targetLine ?? "x"}-${index}`}>
                        <span className="diff-marker">{diffLineMarker(line.kind)}</span>
                        <span className="diff-line-number">{diffLineNumberLabel(line)}</span>
                        <span className="diff-text">{line.text}</span>
                      </code>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
      </>
    );
  }

  function renderConflictControls(conflict: ConflictItem) {
    const action = selectedConflictAction(conflict);
    const target = selectedConflictTarget(conflict);
    return (
      <>
        <div className="conflict-action-row">
          <select
            aria-label="Conflict action"
            value={action}
            onChange={(event) =>
              setConflictActions((actions) => ({
                ...actions,
                [conflict.id]: event.target.value as ConflictResolutionAction,
              }))
            }
          >
            {safeConflictActions(conflict).map((option) => (
              <option value={option} key={option}>{actionLabel(option)}</option>
            ))}
          </select>
          <small>Selected: {actionLabel(action)}</small>
        </div>
        {action === "rename" ? (
          <div className="conflict-target-box">
            <input
              aria-label="Rename target"
              value={target}
              onChange={(event) =>
                setConflictTargets((targets) => ({
                  ...targets,
                  [conflict.id]: event.target.value,
                }))
              }
              placeholder={conflict.targetRelativePath || "target-relative-path.md"}
            />
            {conflict.renameSuggestions?.length ? (
              <div className="conflict-suggestion-row">
                {conflict.renameSuggestions.slice(0, 3).map((suggestion) => (
                  <button
                    type="button"
                    className="ghost-button"
                    key={suggestion.targetRelativePath}
                    onClick={() =>
                      setConflictTargets((targets) => ({
                        ...targets,
                        [conflict.id]: suggestion.targetRelativePath,
                      }))
                    }
                  >
                    {suggestion.targetRelativePath}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
      </>
    );
  }

  async function resolveConflict(conflict: ConflictItem, action: ConflictResolutionAction) {
    const result = await run(
      () => commands.resolveConflict(vaultPath, conflict.id, action),
      "处理冲突",
      "冲突已处理",
    );
    if (!result) return;
    if (result.resolved) {
      setConflicts((items) => items.filter((item) => item.id !== conflict.id));
    }
  }

  async function recordConflictRule(conflict: ConflictItem) {
    const answer = (conflictAnswers[conflict.id] ?? "").trim();
    const action = selectedConflictAction(conflict);
    const targetRelativePath = action === "rename" ? selectedConflictTarget(conflict).trim() : conflict.targetRelativePath;
    if (!answer) {
      setAppError({
        title: "规则答案为空",
        detail: "请先写下这类冲突以后应该如何处理。",
        recovery: "第一版只记录本地可读规则，不会自动覆盖或删除文件。",
      });
      return;
    }
    if (action === "rename" && !targetRelativePath) {
      setAppError({
        title: "Rename target is empty",
        detail: "Choose a suggested target or type a Vault-relative target path first.",
        recovery: "The frontend will not auto-apply a rename without an explicit target.",
      });
      return;
    }
    const result = await run(
      () =>
        commands.submitConflictAnswer(vaultPath, {
          conflictId: conflict.id,
          answer,
          action,
          targetRelativePath,
          autoApply: false,
          matchSummary: `${action} for ${conflict.targetRelativePath || conflict.sourceRelativePath}`,
        }),
      "记录冲突规则",
      "冲突规则已记录",
    );
    if (!result || result.isFallback) return;
    setConflictAnswers((answers) => ({ ...answers, [conflict.id]: "" }));
    await refreshOperations(vaultPath);
  }

  async function applyRecommendedRule(conflict: ConflictItem, ruleId: number) {
    const result = await run(
      () => commands.applyConflictRule(vaultPath, conflict.id, ruleId),
      "应用冲突规则",
      "冲突规则已确认",
    );
    if (!result || result.isFallback) return;
    await refreshOperations(vaultPath);
  }

  async function toggleConflictRule(rule: ConflictRule) {
    if (!vaultPath) return;
    const nextStatus = rule.status === "active" ? "disabled" : "active";
    const result = await run(
      () => commands.setConflictRuleStatus(vaultPath, rule.id, nextStatus),
      nextStatus === "active" ? "启用冲突规则" : "禁用冲突规则",
      nextStatus === "active" ? "规则已启用" : "规则已禁用",
    );
    if (!result || result.isFallback) return;
    await refreshOperations(vaultPath);
  }

  async function saveConflictRuleEdit(rule: ConflictRule) {
    if (!vaultPath) return;
    const draft = conflictRuleDraft(rule);
    const result = await run(
      () =>
        commands.updateConflictRule(vaultPath, {
          ruleId: rule.id,
          answer: draft.answer,
          action: draft.action,
          targetPattern: draft.targetPattern,
          autoApply: draft.autoApply,
          matchSummary: draft.matchSummary,
        }),
      "保存冲突规则",
      "冲突规则已保存",
    );
    if (!result || result.isFallback) return;
    setConflictRuleDrafts((drafts) => {
      const next = { ...drafts };
      delete next[rule.id];
      return next;
    });
    await refreshOperations(vaultPath);
  }

  function startNewRagConversation() {
    setActiveConversationId(null);
    setRagMessages([]);
    setRagAnswer(null);
    setRagTrace(null);
    setRagHistoryNotice("");
    setActiveView("dashboard");
    setStatus("已准备新 RAG 对话");
  }

  async function loadRagConversation(conversationId: number) {
    if (!vaultPath || !conversationId) return;
    const result = await run(
      () => commands.getRagConversation(vaultPath, conversationId),
      "读取 RAG 对话",
      "RAG 对话已打开",
    );
    if (!result || result.isFallback) {
      setRagHistoryNotice(result?.fallbackReason ? `会话历史未连接：${result.fallbackReason}` : "会话历史未连接。");
      return;
    }
    setActiveConversationId(result.conversation.id);
    setRagMessages(result.messages);
    setRagConversations((current) => {
      const withoutCurrent = current.filter((conversation) => conversation.id !== result.conversation.id);
      return [result.conversation, ...withoutCurrent];
    });
    setRagHistoryNotice("");
    setActiveView("dashboard");
  }

  function ragScopePrefix(kind: "dashboard" | "project"): string {
    if (kind === "project") return selectedProject?.relativePath ?? "";
    return currentDirectoryPrefix || selectedProject?.relativePath || "";
  }

  function resolveRagScope(kind: "dashboard" | "project"): { scope: RagScope; label: string; detail: string } {
    if (ragScopeKind === "currentFile" && currentMarkdownRelativePath) {
      return {
        scope: { kind: "currentFile", relativePath: currentMarkdownRelativePath },
        label: "当前文件",
        detail: currentMarkdownRelativePath,
      };
    }
    const prefix = ragScopePrefix(kind);
    if (ragScopeKind === "projectPrefix" && prefix) {
      return {
        scope: { kind: "projectPrefix", relativePathPrefix: prefix },
        label: kind === "project" ? "当前项目" : currentDirectoryPrefix ? "当前目录" : "项目前缀",
        detail: prefix,
      };
    }
    return { scope: { kind: "allVault" }, label: "全库", detail: "整个 Vault" };
  }

  async function submitPrompt(prompt: string, scope: string, kind: "dashboard" | "project"): Promise<boolean> {
    const trimmed = prompt.trim();
    if (!trimmed || ragIsAsking) return false;
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "RAG 问答需要先读取本地 Vault 索引。",
        recovery: "选择并初始化 Vault，然后重建 RAG 索引。",
      });
      return false;
    }
    const ragScope = resolveRagScope(kind);
    setRagIsAsking(true);
    try {
      let conversationId = activeConversationId;
      if (!conversationId) {
        const created = await run(
          () => commands.createRagConversation(vaultPath, trimmed.slice(0, 36)),
          "创建 RAG 对话",
          "RAG 对话已创建",
        );
        if (!created || created.isFallback || !created.id) {
          setRagHistoryNotice(
            created?.fallbackReason
              ? `RAG 会话未保存：${created.fallbackReason}`
              : "RAG 会话未保存，未继续提交问题。",
          );
          return false;
        }
        conversationId = created.id;
        setActiveConversationId(conversationId);
        setRagConversations((current) => [created, ...current.filter((item) => item.id !== created.id)]);
        setRagMessages([]);
        setRagHistoryNotice("");
      }
      const savedConversationId = conversationId;
      if (!savedConversationId) return false;

      const result = await run(
        () => commands.askRag(vaultPath, trimmed, 6, savedConversationId, ragScope.scope),
        `${scope} RAG 问答（范围：${ragScope.label}）`,
        "RAG 已回答",
      );
      if (!result) return false;
      setRagAnswer(result);
      setRagTrace(result.trace);
      if (result.isFallback) {
        setRagHistoryNotice("本次回答是 fallback，不能确认已写入 RAG 会话历史。");
        return true;
      }
      const [detail, conversations] = await Promise.all([
        commands.getRagConversation(vaultPath, savedConversationId),
        commands.listRagConversations(vaultPath, 20),
      ]);
      setRagConversations(conversations);
      if (detail.isFallback) {
        setRagHistoryNotice(
          detail.fallbackReason ? `回答已返回，但会话详情未刷新：${detail.fallbackReason}` : "回答已返回，但会话详情未刷新。",
        );
      } else {
        setActiveConversationId(detail.conversation.id);
        setRagMessages(detail.messages);
        setRagHistoryNotice("");
      }
      return true;
    } finally {
      setRagIsAsking(false);
    }
  }

  function queueIssueMessage(item: QueueItem): string {
    const payload = item.payload ?? {};
    const message =
      payload.lastError ??
      payload.error ??
      payload.reason ??
      payload.message ??
      payload.skippedReason ??
      "未记录具体原因";
    const metadata = decisionMetadataFromPayload(payload);
    const metadataSummary = decisionMetadataSummary(metadata);
    const reasonLabels = metadata?.confirmationReasons.map(confirmationReasonLabel) ?? [];
    return [String(message), metadataSummary, reasonLabels.length > 0 ? `确认原因：${reasonLabels.join(" / ")}` : undefined]
      .filter(Boolean)
      .join(" / ");
  }

  function auditPayloadSummary(event: AuditEvent): string {
    const payload = event.payload ?? {};
    const parts = [
      payload.sourceRelativePath,
      payload.targetRelativePath ? `-> ${payload.targetRelativePath}` : undefined,
      payload.status,
      payload.reason,
      payload.message,
    ]
      .filter(Boolean)
      .map(String);
    return parts.length > 0 ? parts.join(" · ") : "无详细摘要";
  }

  function renderQueueRecoveryPreview() {
    if (!batchRecoveryPreview || batchRecoveryPreview.kind === "rollback") return null;
    const preview = batchRecoveryPreview.preview;
    const eligibleCount = preview.items.filter((item) => item.eligible).length;
    return (
      <div className="batch-preview">
        <div>
          <strong>{preview.action === "skip" ? "批量跳过预览" : "批量重试预览"}</strong>
          <span>尚未执行；确认后只会处理 {eligibleCount} / {preview.requested} 个 eligible 队列项。</span>
          {preview.failed.length > 0 ? <small>读取失败 {preview.failed.length} 项，确认时不会处理这些项。</small> : null}
        </div>
        <div className="batch-preview-list">
          {preview.items.slice(0, 6).map((item) => (
            <article key={item.id} className={item.eligible ? undefined : "is-blocked"}>
              <span>#{item.id} {item.currentStatus}</span>
              <strong>{item.relativePath}</strong>
              <small>{item.eligible ? item.effect : item.blockingReason}</small>
              {item.warnings.length > 0 ? <em>{item.warnings.join(" · ")}</em> : null}
            </article>
          ))}
        </div>
        <div className="recovery-action-row">
          <button type="button" className="secondary-button" onClick={() => confirmQueueRecoveryPreview(preview)} disabled={eligibleCount === 0}>
            确认执行
          </button>
          <button type="button" className="ghost-button" onClick={() => setBatchRecoveryPreview(null)}>
            取消预览
          </button>
        </div>
      </div>
    );
  }

  function renderRollbackPreview() {
    if (!batchRecoveryPreview || batchRecoveryPreview.kind !== "rollback") return null;
    const preview = batchRecoveryPreview.preview;
    const eligibleCount = preview.items.filter((item) => item.eligible).length;
    return (
      <div className="batch-preview">
        <div>
          <strong>批量回滚预览</strong>
          <span>尚未执行；确认后只会回滚 {eligibleCount} / {preview.requested} 条 eligible movement log。</span>
          {preview.failed.length > 0 ? <small>读取失败 {preview.failed.length} 条，确认时不会处理这些项。</small> : null}
        </div>
        <div className="batch-preview-list">
          {preview.items.slice(0, 6).map((item) => (
            <article key={item.id} className={item.eligible ? undefined : "is-blocked"}>
              <span>#{item.id} {item.status}</span>
              <strong>{item.currentFileRelativePath} {"->"} {item.restoreRelativePath}</strong>
              <small>{item.eligible ? "将移动回原收集箱路径并追加 ledger" : item.blockingReason}</small>
              {item.warnings.length > 0 ? <em>{item.warnings.join(" · ")}</em> : null}
            </article>
          ))}
        </div>
        <div className="recovery-action-row">
          <button type="button" className="secondary-button" onClick={() => confirmRollbackPreview(preview)} disabled={eligibleCount === 0}>
            确认回滚
          </button>
          <button type="button" className="ghost-button" onClick={() => setBatchRecoveryPreview(null)}>
            取消预览
          </button>
        </div>
      </div>
    );
  }

  function renderInboxStatusPanel() {
    const issueItems = recoveryQueueItems.slice(0, 6);
    const recentEvent =
      listenerStatus?.lastEvent ??
      queueStatus?.lastEvent ??
      workerRunResult?.items[0]?.message ??
      "暂无事件";
    const recentError =
      residentWorkerStatus?.lastError ??
      listenerStatus?.lastError ??
      (queueStatus?.items?.failed?.[0] ? queueIssueMessage(queueStatus.items.failed[0]) : undefined);
    const featuredConflict = conflicts[0];
    return (
      <section className="concept-card inbox-status-panel">
        <div className="card-heading">
          <h3>
            <Activity size={18} aria-hidden="true" />
            整理状态面板
          </h3>
          <button type="button" className="ghost-button" onClick={() => refresh(vaultPath)} disabled={!vaultPath}>
            <RefreshCw size={16} aria-hidden="true" />
            刷新
          </button>
        </div>
        <div className="status-metric-grid">
          <div>
            <span>Listener</span>
            <strong>{listenerStatus?.status ?? "未连接"}</strong>
            <small>{listenerStatus?.watchPath ?? "未绑定路径"}</small>
          </div>
          <div>
            <span>Queue</span>
            <strong>{queueStateLabel(queueStatus?.state)}</strong>
            <small>
              pending {queueStatus?.pending ?? 0} · failed {queueStatus?.failed ?? 0} · conflict {queueStatus?.conflicts ?? 0} · completed {queueStatus?.completedToday ?? 0}
            </small>
          </div>
          <div>
            <span>Worker</span>
            <strong>{workerStatus?.listener.status ?? "未连接"}</strong>
            <small>running {workerStatus?.running ?? 0} · failed {workerStatus?.failed ?? 0}</small>
          </div>
          <div>
            <span>Resident</span>
            <strong>{residentWorkerStatus?.running ? "运行中" : residentWorkerStatus?.lastStatus ?? "未启动"}</strong>
            <small>
              tick {residentWorkerStatus?.maxItemsPerTick ?? 1} · moved {residentWorkerStatus?.lastMoved ?? 0} · failed {residentWorkerStatus?.lastFailed ?? 0}
            </small>
          </div>
          <div>
            <span>Movement</span>
            <strong>{moveLogs.filter((log) => log.status === "moved").length}</strong>
            <small>可回滚移动记录</small>
          </div>
          <div>
            <span>Archive Map</span>
            <strong>{archiveMapHealthLabel(archiveMap?.health?.status ?? archiveMap?.run.status)}</strong>
            <small>
              目录 {archiveMap?.health?.currentDirectoryCount ?? archiveMap?.directoryCount ?? 0} · stale {archiveMap?.health?.staleDirectoryCount ?? 0} · 历史归档命中 {(archiveMap?.topHitDirectories ?? []).reduce((total, item) => total + item.moveCount, 0)}
            </small>
          </div>
        </div>
        <div className="status-event-grid">
          <div>
            <span>最近事件</span>
            <strong>{recentEvent}</strong>
            <small>{listenerStatus?.lastEventAt ?? queueStatus?.updatedAt ?? "未记录时间"}</small>
          </div>
          <div>
            <span>最近错误</span>
            <strong>{recentError ?? "暂无错误"}</strong>
            <small>{listenerStatus?.fallbackReason ?? queueStatus?.fallbackReason ?? "真实命令状态"}</small>
          </div>
        </div>
        {recoveryQueueItems.length > 0 ? (
          <div className="recovery-action-row">
            <button type="button" className="secondary-button" onClick={() => previewQueueRecovery("retry")} disabled={!vaultPath}>
              预览批量重试
            </button>
            <button type="button" className="ghost-button" onClick={() => previewQueueRecovery("skip")} disabled={!vaultPath}>
              预览批量跳过
            </button>
          </div>
        ) : null}
        {renderQueueRecoveryPreview()}
        {issueItems.length > 0 ? (
          <div className="queue-issue-list">
            {issueItems.map((item) => (
              <article key={item.id}>
                <div>
                  <strong>#{item.id} {item.relativePath}</strong>
                  <span>{item.status} · attempts {item.attempts}/{item.maxAttempts}</span>
                  <small>{queueIssueMessage(item)}</small>
                </div>
                <button type="button" className="secondary-button" onClick={() => retryQueueItem(item)}>
                  重试
                </button>
                <button type="button" className="ghost-button" onClick={() => skipQueueItem(item)}>
                  跳过
                </button>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">暂无失败或冲突队列项</p>
        )}
        {featuredConflict ? (
          <div className="status-conflict-card">
            <div>
              <span>待处理冲突</span>
              <strong>{featuredConflict.sourceRelativePath || "unknown source"}</strong>
              <small>{featuredConflict.message} · 目标 {featuredConflict.targetRelativePath || "未给出"}</small>
              {renderConflictDetails(featuredConflict)}
              {renderConflictControls(featuredConflict)}
              <textarea
                aria-label="状态面板冲突规则答案"
                value={conflictAnswers[featuredConflict.id] ?? ""}
                onChange={(event) =>
                  setConflictAnswers((answers) => ({
                    ...answers,
                    [featuredConflict.id]: event.target.value,
                  }))
                }
                placeholder="记录这类冲突以后应该如何处理"
                rows={2}
              />
            </div>
            {featuredConflict.recommendations?.[0] ? (
              <button
                type="button"
                onClick={() => applyRecommendedRule(featuredConflict, featuredConflict.recommendations![0].rule.id)}
              >
                应用推荐
              </button>
            ) : null}
            <button type="button" className="secondary-button" onClick={() => recordConflictRule(featuredConflict)}>
              记录规则
            </button>
            <button type="button" className="ghost-button" onClick={() => resolveConflict(featuredConflict, selectedConflictAction(featuredConflict))}>
              {selectedConflictAction(featuredConflict) === "skip" ? "跳过冲突" : "提交选择"}
            </button>
          </div>
        ) : null}
      </section>
    );
  }

  function renderMovementLogPanel() {
    const recentLogs = moveLogs.slice(0, 10);
    return (
      <section className="concept-card movement-log-card">
        <div className="card-heading">
          <h3>
            <RotateCcw size={18} aria-hidden="true" />
            Movement log
          </h3>
          <div className="card-actions">
            <span>{moveLogs.length} 条记录</span>
            <button
              type="button"
              className="secondary-button"
              onClick={previewVisibleMoveLogRollback}
              disabled={!vaultPath || rollbackableMoveLogs.length === 0}
            >
              预览批量回滚
            </button>
          </div>
        </div>
        {renderRollbackPreview()}
        {recentLogs.length > 0 ? (
          <div className="movement-log-list">
            {recentLogs.map((log) => (
              <article key={log.id}>
                <div>
                  <strong>#{log.id} {log.status}</strong>
                  <span>{log.sourceRelativePath} {"->"} {log.targetRelativePath}</span>
                  <small>
                    {formatDate(log.movedAt ?? log.createdAt)}
                    {log.error ? ` · ${log.error}` : ""}
                  </small>
                </div>
                <button
                  type="button"
                  className="secondary-button"
                  onClick={() => rollbackMoveLog(log)}
                  disabled={!vaultPath || log.status !== "moved"}
                >
                  回滚
                </button>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">暂无移动记录</p>
        )}
      </section>
    );
  }

  function renderAuditTimelinePanel() {
    const recentEvents = auditEvents;
    const filterTypes = auditEventTypeFilter && !auditEventTypes.includes(auditEventTypeFilter)
      ? [auditEventTypeFilter, ...auditEventTypes]
      : auditEventTypes;
    return (
      <section className="concept-card audit-timeline-card">
        <div className="card-heading">
          <h3>
            <Clock3 size={18} aria-hidden="true" />
            Audit timeline
          </h3>
          <span>{auditEvents.length} 条最近事件</span>
        </div>
        <div className="audit-filter-row">
          <input
            value={auditSearchText}
            onChange={(event) => setAuditSearchText(event.target.value)}
            placeholder="搜索事件、路径、状态或 payload"
            aria-label="搜索 audit timeline"
          />
          <select
            value={auditEventTypeFilter}
            onChange={(event) => setAuditEventTypeFilter(event.target.value)}
            aria-label="按 audit 类型筛选"
          >
            <option value="">全部类型</option>
            {filterTypes.map((eventType) => (
              <option value={eventType} key={eventType}>{eventType}</option>
            ))}
          </select>
          <label>
            <span>开始</span>
            <input
              type="date"
              value={auditStartDate}
              onChange={(event) => setAuditStartDate(event.target.value)}
              aria-label="audit 开始日期"
            />
          </label>
          <label>
            <span>结束</span>
            <input
              type="date"
              value={auditEndDate}
              onChange={(event) => setAuditEndDate(event.target.value)}
              aria-label="audit 结束日期"
            />
          </label>
          <button type="button" className="secondary-button" onClick={applyAuditSearch} disabled={!vaultPath}>
            应用筛选
          </button>
          <button type="button" className="ghost-button" onClick={resetAuditSearch} disabled={!vaultPath}>
            重置
          </button>
        </div>
        <p className="insights-note muted">筛选结果来自 `.thebrain/index.sqlite` audit_events；当前已加载 {auditEvents.length} 条。</p>
        {recentEvents.length > 0 ? (
          <div className="audit-timeline-list">
            {recentEvents.map((event) => {
              const isExpanded = expandedAuditEventId === event.id;
              return (
                <article className={isExpanded ? "is-expanded" : undefined} key={event.id}>
                  <div>
                    <strong>{event.eventType}</strong>
                    <span>{auditPayloadSummary(event)}</span>
                  </div>
                  <small>{formatDate(event.createdAt)}</small>
                  <button
                    type="button"
                    className="ghost-button"
                    onClick={() => setExpandedAuditEventId(isExpanded ? null : event.id)}
                  >
                    {isExpanded ? "收起" : "详情"}
                  </button>
                  {isExpanded ? (
                    <code>{boundedJson(event.payload)}</code>
                  ) : null}
                </article>
              );
            })}
          </div>
        ) : (
          <p className="empty-state">暂无 audit 事件</p>
        )}
        <div className="audit-pagination-row">
          <button type="button" className="secondary-button" onClick={loadMoreAuditEvents} disabled={!vaultPath || !auditNextBeforeId}>
            加载更多
          </button>
          <span>{auditNextBeforeId ? `下一页 before #${auditNextBeforeId}` : "没有更多已加载游标"}</span>
        </div>
      </section>
    );
  }

  function renderConflictRulePanel() {
    const recentRules = conflictRules.slice(0, 4);
    return (
      <section className="concept-card conflict-rule-panel">
        <div className="card-heading">
          <h3>冲突规则记忆</h3>
          <span>{conflictRules.length}</span>
        </div>
        {recentRules.length > 0 ? (
          <div className="conflict-rule-list">
            {recentRules.map((rule) => {
              const draft = conflictRuleDraft(rule);
              return (
                <article key={rule.id} className={rule.status === "active" ? undefined : "is-disabled"}>
                  <div>
                    <strong>#{rule.id} {rule.status}</strong>
                    <span>{rule.sourcePattern}</span>
                    <small>hits {rule.hitCount} · {formatDate(rule.updatedAt)}</small>
                  </div>
                  <label>
                    <span>动作</span>
                    <select
                      value={draft.action}
                      onChange={(event) =>
                        updateConflictRuleDraft(rule.id, { action: event.target.value as ConflictResolutionAction })
                      }
                    >
                      {(["rename", "skip", "keep_existing"] as ConflictResolutionAction[]).map((action) => (
                        <option value={action} key={action}>{actionLabel(action)}</option>
                      ))}
                    </select>
                  </label>
                  <label>
                    <span>目标</span>
                    <input
                      value={draft.targetPattern}
                      onChange={(event) => updateConflictRuleDraft(rule.id, { targetPattern: event.target.value })}
                    />
                  </label>
                  <label>
                    <span>答案</span>
                    <textarea
                      value={draft.answer}
                      onChange={(event) => updateConflictRuleDraft(rule.id, { answer: event.target.value })}
                      rows={2}
                    />
                  </label>
                  <label>
                    <span>匹配说明</span>
                    <input
                      value={draft.matchSummary}
                      onChange={(event) => updateConflictRuleDraft(rule.id, { matchSummary: event.target.value })}
                    />
                  </label>
                  <div className="recovery-action-row">
                    <button type="button" className="secondary-button" onClick={() => saveConflictRuleEdit(rule)}>
                      保存修改
                    </button>
                    <button type="button" className="ghost-button" onClick={() => toggleConflictRule(rule)}>
                      {rule.status === "active" ? "禁用" : "启用"}
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        ) : (
          <p className="empty-state">暂无冲突规则</p>
        )}
      </section>
    );
  }

  function handleDragEnter(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    setDragActive(true);
  }

  function handleDragOver(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    event.dataTransfer.dropEffect = importBehavior === "move" ? "move" : "copy";
    setDragActive(true);
  }

  function handleDragLeave(event: React.DragEvent<HTMLElement>) {
    if (event.currentTarget === event.target) {
      setDragActive(false);
    }
  }

  async function handleDrop(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    const count = event.dataTransfer.files.length;
    setDragActive(false);
    setActiveView("inbox");
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (paths.length > 0) {
      await importFiles(paths);
    } else {
      setStatus(count > 0 ? "拖拽文件缺少本地路径，请使用导入文件按钮" : "拖拽导入层已打开");
    }
  }

  function renderVaultTree(nodes: VaultTreeNode[], depth = 0): ReactNode {
    if (nodes.length === 0 && depth === 0) {
      return <p className="sidebar-empty">选择并初始化 Vault 后显示目录树</p>;
    }

    return nodes.map((node) => {
      const isMarkdown = /\.(md|markdown)$/i.test(node.relativePath);
      const isExpanded = node.isDir && expandedTreePaths.has(node.relativePath);
      const hasChildren = node.children.length > 0;
      return (
        <div className="tree-node" key={node.relativePath || node.name}>
          <button
            type="button"
            className={[
              "tree-row",
              node.isDir ? "folder-row" : "",
              node.relativePath === relativePath ? "active" : "",
            ].filter(Boolean).join(" ")}
            style={{ paddingLeft: `${10 + depth * 14}px` }}
            aria-expanded={node.isDir && hasChildren ? isExpanded : undefined}
            title={node.relativePath}
            onClick={() => {
              if (node.isDir) {
                if (hasChildren) {
                  toggleTreeFolder(node.relativePath);
                }
                return;
              }
              if (isMarkdown) {
                void openMarkdown(node.relativePath);
              }
            }}
          >
            {node.isDir && hasChildren ? (
              isExpanded ? <ChevronDown size={13} aria-hidden="true" /> : <ChevronRight size={13} aria-hidden="true" />
            ) : (
              <span className="tree-indent" />
            )}
            {node.isDir ? (
              isExpanded ? <FolderOpen size={15} aria-hidden="true" /> : <Folder size={15} aria-hidden="true" />
            ) : fileIcon(node.relativePath)}
            <span className="tree-label">{node.name}</span>
          </button>
          {node.isDir && hasChildren && isExpanded ? (
            <div className="tree-children">{renderVaultTree(node.children, depth + 1)}</div>
          ) : null}
        </div>
      );
    });
  }

  function renderLeftSidebar() {
    return (
      <aside className="sidebar" aria-label="主导航">
        <div className="brand-row">
          <div className="brand-mark">
            <HardDrive size={17} aria-hidden="true" />
          </div>
          <strong>TheBrain</strong>
          <button type="button" className="sidebar-collapse" title="折叠侧栏">
            <ChevronDown size={16} aria-hidden="true" />
          </button>
        </div>

        <section className="sidebar-section">
          <h2>基础功能</h2>
          <nav className="nav-stack" aria-label="基础功能">
            {basicNavItems.map((item) => (
              <button
                type="button"
                key={item.id}
                className={activeView === item.id ? "nav-item active" : "nav-item"}
                onClick={() => setActiveView(item.id)}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </section>

        <section className="sidebar-section">
          <div className="section-title-row">
            <h2>项目</h2>
            <button type="button" className="tiny-icon-button" title="新建项目" onClick={() => setActiveView("project")}>
              <Plus size={14} aria-hidden="true" />
            </button>
          </div>
          <div className="project-list">
            {projects.map((project) => (
              <button
                type="button"
                key={project.id}
                className={selectedProject?.id === project.id && activeView === "project" ? "project-link active" : "project-link"}
                onClick={() => {
                  setSelectedProjectId(project.id);
                  setActiveView("project");
                }}
              >
                <span>{project.name}</span>
                <small>{project.updatedAt}</small>
              </button>
            ))}
          </div>
        </section>

        <section className="sidebar-section vault-tree-section">
          <h2>Vault</h2>
          <div className="vault-actions">
            <button type="button" className="mini-button" onClick={chooseVault}>
              <FolderOpen size={14} aria-hidden="true" />
              选择
            </button>
            <button type="button" className="mini-button" onClick={initVault} disabled={!vaultPath}>
              <Check size={14} aria-hidden="true" />
              初始化
            </button>
          </div>
          <div className="vault-root">
            <button type="button" className="vault-root-row" onClick={() => setActiveView("inbox")}>
              <Inbox size={15} aria-hidden="true" />
              <span>000-收集箱</span>
              <small>{inboxFiles.length}</small>
            </button>
            <div className="tree-scroll">{renderVaultTree(tree)}</div>
          </div>
        </section>

        <section className="sidebar-section conversation-section">
          <div className="section-title-row">
            <h2>RAG 会话</h2>
            <button type="button" className="tiny-icon-button" title="新对话" onClick={startNewRagConversation}>
              <Plus size={14} aria-hidden="true" />
            </button>
          </div>
          <div className="conversation-list">
            {ragConversations.map((conversation) => (
              <button
                type="button"
                key={conversation.id}
                className={conversation.id === activeConversationId ? "active" : undefined}
                onClick={() => void loadRagConversation(conversation.id)}
              >
                <span>{conversationTitle(conversation)}</span>
                <small>
                  {conversation.messageCount ? `${conversation.messageCount} 条` : "暂无消息"} · {formatDate(conversation.lastMessageAt ?? conversation.updatedAt)}
                </small>
              </button>
            ))}
            {ragConversations.length === 0 ? <p className="sidebar-empty">暂无 RAG 会话</p> : null}
          </div>
        </section>

        <button type="button" className={activeView === "settings" ? "settings-link active" : "settings-link"} onClick={() => setActiveView("settings")}>
          <Settings size={17} aria-hidden="true" />
          设置
        </button>
      </aside>
    );
  }

  function renderTopbar() {
    return (
      <header className="topbar">
        <div>
          <h1>{page.title}</h1>
          <p>{page.subtitle}</p>
        </div>
        <div className="topbar-actions">
          <button type="button" className="ghost-button" onClick={() => refresh()}>
            <RefreshCw size={15} aria-hidden="true" />
            刷新
          </button>
          <span className="sync-pill">
            <Circle size={9} aria-hidden="true" />
            同步状态 · 5 分钟前
          </span>
          <button type="button" className="avatar-button" title="个人页" onClick={() => setActiveView("personal")}>
            <UserRound size={18} aria-hidden="true" />
          </button>
        </div>
      </header>
    );
  }

  function renderPromptBox(kind: "dashboard" | "project") {
    const value = kind === "dashboard" ? chatPrompt : projectPrompt;
    const setValue = kind === "dashboard" ? setChatPrompt : setProjectPrompt;
    const scopeSummary = resolveRagScope(kind);
    const prefixAvailable = Boolean(ragScopePrefix(kind));
    const prefixLabel = kind === "project" ? "当前项目" : currentDirectoryPrefix ? "当前目录" : "项目前缀";
    const placeholder =
      kind === "dashboard"
        ? "输入你的问题或想法，按 Enter 发送，Shift + Enter 换行"
        : `询问关于 ${selectedProject?.name ?? "项目"} 的问题，或让 Agent 帮你推进项目...`;
    const submitCurrentPrompt = async () => {
      const submitted = await submitPrompt(value, kind === "dashboard" ? "主对话" : "项目对话", kind);
      if (submitted) setValue("");
    };
    return (
      <div className={kind === "dashboard" ? "prompt-box hero-prompt" : "prompt-box"}>
        <textarea
          value={value}
          placeholder={placeholder}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (ragIsAsking) return;
              void submitCurrentPrompt();
            }
          }}
        />
        <div className="prompt-footer">
          <div className="prompt-tools">
            <button type="button" className="icon-soft-button" title="添加资料" onClick={() => setActiveView("inbox")}>
              <Plus size={17} aria-hidden="true" />
            </button>
            <button type="button" className="chip-button">
              <Sparkles size={15} aria-hidden="true" />
              智能模式
              <ChevronDown size={14} aria-hidden="true" />
            </button>
            <div className="scope-segmented" aria-label="RAG 问答范围">
              <button
                type="button"
                className={ragScopeKind === "allVault" ? "active" : undefined}
                onClick={() => setRagScopeKind("allVault")}
                title="在整个 Vault 中检索"
              >
                全库
              </button>
              <button
                type="button"
                className={ragScopeKind === "currentFile" ? "active" : undefined}
                onClick={() => setRagScopeKind("currentFile")}
                disabled={!currentMarkdownRelativePath}
                title={currentMarkdownRelativePath || "当前没有可用 Markdown 文件"}
              >
                当前文件
              </button>
              <button
                type="button"
                className={ragScopeKind === "projectPrefix" ? "active" : undefined}
                onClick={() => setRagScopeKind("projectPrefix")}
                disabled={!prefixAvailable}
                title={ragScopePrefix(kind) || "当前没有可用目录或项目前缀"}
              >
                {prefixLabel}
              </button>
            </div>
          </div>
          <div className="prompt-tools">
            <button type="button" className="chip-button" title={scopeSummary.detail}>范围：{scopeSummary.label}</button>
            <button type="button" className="chip-button">RAG · MiMo</button>
            <button type="button" className="chip-button api-connected">
              <Circle size={8} aria-hidden="true" />
              {mimoKeyLabel}
            </button>
            <button type="button" className="send-button" onClick={() => void submitCurrentPrompt()} disabled={ragIsAsking}>
              <Send size={18} aria-hidden="true" />
            </button>
          </div>
        </div>
      </div>
    );
  }

  function renderRagPanel() {
    const lastRun = ragStatus?.lastRun ?? ragIndexRun;
    const trace = ragTrace ?? ragAnswer?.trace ?? null;
    return (
      <section className="rag-panel">
        <article className="concept-card rag-answer-card">
          <div className="card-heading">
            <h3>
              <Bot size={18} aria-hidden="true" />
              长期记忆问答
            </h3>
            <button type="button" className="ghost-button" onClick={rebuildRagIndex} disabled={!vaultPath}>
              <RefreshCw size={15} aria-hidden="true" />
              重建索引
            </button>
          </div>
          <div className="rag-stats">
            <div>
              <span>文档</span>
              <strong>{ragStatus?.documentCount ?? 0}</strong>
            </div>
            <div>
              <span>Chunks</span>
              <strong>{ragStatus?.chunkCount ?? 0}</strong>
            </div>
            <div>
              <span>Schema</span>
              <strong>v{ragStatus?.schemaVersion ?? 0}</strong>
            </div>
            <div>
              <span>最近运行</span>
              <strong>{lastRun?.status ?? "未运行"}</strong>
            </div>
          </div>
          {lastRun ? (
            <div className="rag-run-line">
              <span>扫描 {lastRun.scannedCount}</span>
              <span>更新 {lastRun.indexedCount}</span>
              <span>跳过 {lastRun.skippedCount}</span>
              <span>删除 {lastRun.deletedCount}</span>
            </div>
          ) : null}
          <div className="rag-conversation-bar">
            <div>
              <span>当前会话</span>
              <strong>{activeConversation ? conversationTitle(activeConversation) : "新 RAG 对话"}</strong>
            </div>
            <button type="button" className="text-link-button" onClick={startNewRagConversation}>
              新对话
            </button>
          </div>
          {ragHistoryNotice ? <p className="rag-history-notice">{ragHistoryNotice}</p> : null}
          <div className="rag-message-list" aria-label="当前 RAG 会话消息">
            {ragMessages.map((message) => (
              <div className={`rag-message ${message.role === "user" ? "user" : "assistant"}`} key={message.id}>
                <span>{messageRoleLabel(message.role)}</span>
                <p>{message.content}</p>
                <small>
                  {formatDate(message.createdAt)}
                  {message.status ? ` · ${message.status}` : ""}
                  {message.isMock || message.isFallback ? " · fallback" : ""}
                </small>
              </div>
            ))}
            {activeConversationId && ragMessages.length === 0 ? <p className="empty-state">当前会话暂无已保存消息</p> : null}
            {!activeConversationId ? <p className="empty-state">提问后会先创建 RAG 会话并保存历史</p> : null}
          </div>
          {ragIsAsking ? (
            <div className="rag-answer rag-answer-loading">
              <div className="rag-answer-meta">
                <span>MiMo 正在回答</span>
                <span>后台运行中</span>
              </div>
              <p>正在检索本地引用并等待 AI 响应。你可以继续浏览 Vault 或处理其他内容。</p>
            </div>
          ) : ragAnswer ? (
            <div className="rag-answer">
              <div className="rag-answer-meta">
                <span>{ragAnswer.provider} · {ragAnswer.model}</span>
                <span>{ragAnswer.status}</span>
                {ragAnswer.isMock ? <span>fallback</span> : null}
              </div>
              <p>{ragAnswer.answer}</p>
              {ragAnswer.fallbackReason ? <small>{ragAnswer.fallbackReason}</small> : null}
            </div>
          ) : (
            <p className="empty-state">暂无 RAG 回答</p>
          )}
          <div className="rag-citations">
            {(ragAnswer?.citations ?? []).map((citation) => {
              const canOpen = /\.(md|markdown)$/i.test(citation.relativePath);
              return (
                <button
                  type="button"
                  key={`${citation.id}-${citation.relativePath}-${citation.startLine}`}
                  onClick={() => {
                    if (canOpen) {
                      void openMarkdown(citation.relativePath);
                    } else {
                      setStatus(`引用文件：${citation.relativePath}`);
                    }
                  }}
                >
                  <strong>{citation.id}</strong>
                  <span>{citation.title}</span>
                  <small>{citation.relativePath} · {citation.startLine}-{citation.endLine} · {percentage(Math.min(1, citation.score))}</small>
                  <em>{citation.snippet}</em>
                </button>
              );
            })}
          </div>
        </article>

        <article className="concept-card rag-trace-card">
          <div className="card-heading">
            <h3>
              <Database size={18} aria-hidden="true" />
              Trace
            </h3>
            <button
              type="button"
              className="text-link-button"
              onClick={async () => {
                if (!vaultPath) return;
                const nextTrace = await commands.getLatestRagTrace(vaultPath);
                setRagTrace(nextTrace);
              }}
            >
              刷新
            </button>
          </div>
          <div className="trace-list">
            {(trace?.nodes ?? []).map((node) => (
              <div key={node.id}>
                <span>{node.nodeType}</span>
                <strong>{node.name}</strong>
                <small>{node.status} · {node.durationMs ?? 0}ms</small>
              </div>
            ))}
            {trace?.nodes.length ? null : <p className="empty-state">暂无 Trace</p>}
          </div>
        </article>
      </section>
    );
  }

  function renderDashboard() {
    return (
      <div className="dashboard-page">
        <section className="dashboard-hero">
          <h2>你好，今天想探索或整理些什么？</h2>
          {renderPromptBox("dashboard")}
        </section>

        {renderRagPanel()}

        <section className="concept-grid three">
          <article className="concept-card">
            <div className="card-heading">
              <h3>
                <Calendar size={18} aria-hidden="true" />
                TODO / 日程
              </h3>
              <span>{activeActionItems.length}</span>
            </div>
            <div className="task-groups">
              {activeActionItems.slice(0, 4).map((item) => (
                <div className="task-line" key={`${item.kind}-${item.id}`}>
                  <input
                    type="checkbox"
                    checked={isCompletedActionItem(item)}
                    aria-label={item.title}
                    onChange={() => completeActionItem(item)}
                  />
                  <span>{item.title}</span>
                  <div className="task-meta">
                    <small>{actionKindLabel(item)} · {actionDateLabel(item)}</small>
                    <button type="button" className="text-link-button" onClick={() => cancelActionItem(item)}>
                      取消
                    </button>
                  </div>
                </div>
              ))}
              {activeActionItems.length === 0 ? (
                <p className="empty-state">暂无正式 TODO/日程；可在收集箱确认候选后创建。</p>
              ) : null}
            </div>
            <button type="button" className="text-link-button" onClick={() => setActiveView("inbox")}>
              添加任务 <ArrowRight size={14} aria-hidden="true" />
            </button>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3>
                <Clock3 size={18} aria-hidden="true" />
                你先前停留的位置
              </h3>
            </div>
            <div className="recent-list">
              {recentMarkdown.slice(0, 3).map((node) => (
                <button type="button" key={node.relativePath} onClick={() => openMarkdown(node.relativePath)}>
                  {fileIcon(node.relativePath)}
                  <span>{node.name}</span>
                  <small>{node.relativePath}</small>
                </button>
              ))}
              {recentMarkdown.length === 0 ? (
                <p className="empty-state">刷新 Vault 后会显示最近 Markdown 文件</p>
              ) : null}
            </div>
            <div className="review-list">
              <strong>推荐复习</strong>
              <span>线性代数基础知识</span>
              <span>Python 常用语法速查</span>
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3>
                <LayoutDashboard size={18} aria-hidden="true" />
                更多功能
              </h3>
            </div>
            <div className="feature-grid">
              <button type="button" onClick={() => setActiveView("inbox")}>
                <Mail size={25} aria-hidden="true" />
                <strong>连接邮箱</strong>
                <span>收集与整理邮件内容</span>
              </button>
              <button type="button" onClick={() => setActiveView("inbox")}>
                <FolderOpen size={25} aria-hidden="true" />
                <strong>连接文件</strong>
                <span>导入本地文件与资料</span>
              </button>
              <button type="button" onClick={planOrganize}>
                <Wand2 size={25} aria-hidden="true" />
                <strong>AI 整理模板</strong>
                <span>智能分类与结构化</span>
              </button>
              <button type="button" onClick={() => setActiveView("settings")}>
                <Zap size={25} aria-hidden="true" />
                <strong>自动化</strong>
                <span>队列、快捷键与预算</span>
              </button>
            </div>
          </article>
        </section>
      </div>
    );
  }

  function renderImportZone(compact = false) {
    return (
      <section className={compact ? "import-zone compact" : "import-zone"}>
        <CloudUpload size={40} aria-hidden="true" />
        <h3>拖入或选择文件到收集箱</h3>
        <p>v0.3 支持 md/txt、mp3/wav/m4a/aac、png/jpg/jpeg；导入、抽取、计划和移动都通过后端受控命令执行。</p>
        <div className="import-type-row">
          {importTypes.map((type) => (
            <span key={type.label}>
              {type.icon}
              {type.label}
            </span>
          ))}
        </div>
      </section>
    );
  }

  function renderInboxPage() {
    const plannedRows = organizePlan?.candidates ?? [];
    const plannedSources = new Set(plannedRows.map((item) => item.sourceRelativePath));
    const queueRows = [
      ...plannedRows,
      ...inboxFiles.filter((item) => !plannedSources.has(item.relativePath)),
    ];
    return (
      <div className="inbox-page">
        <div className="page-action-row">
          <button type="button" className="ghost-button" onClick={() => importFiles()}>
            <Upload size={16} aria-hidden="true" />
            导入文件
          </button>
          <button type="button" className="secondary-button" onClick={scanInboxQueue} disabled={!vaultPath}>
            <RefreshCw size={16} aria-hidden="true" />
            扫描入队
          </button>
          <button type="button" className="secondary-button" onClick={rebuildArchiveMap} disabled={!vaultPath}>
            <Archive size={16} aria-hidden="true" />
            重建归档地图
          </button>
          <button type="button" className="secondary-button" onClick={planOrganize} disabled={!vaultPath}>
            <Sparkles size={16} aria-hidden="true" />
            生成计划
          </button>
          <button type="button" onClick={runOrganize} disabled={!vaultPath}>
            <Play size={16} aria-hidden="true" />
            运行整理
          </button>
          <button type="button" className="secondary-button" onClick={runWorkerControl} disabled={!vaultPath}>
            <Zap size={16} aria-hidden="true" />
            {queueIsPaused ? "恢复并运行 worker" : "运行 worker"}
          </button>
          <button type="button" className="secondary-button" onClick={toggleResidentWorker} disabled={!vaultPath}>
            {residentWorkerIsRunning ? <Pause size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
            {residentWorkerIsRunning ? "停止常驻 worker" : "启动常驻 worker"}
          </button>
          <button type="button" className="secondary-button" onClick={toggleInboxListener} disabled={!vaultPath}>
            {listenerIsRunning ? <Pause size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
            {listenerIsRunning ? "停止监听" : "启动监听"}
          </button>
        </div>

        <section className="inbox-layout">
          <div className="inbox-main">
            {renderImportZone()}
            {renderInboxStatusPanel()}
            <div className="import-options">
              <label>
                <input
                  type="radio"
                  checked={importBehavior === "copy"}
                  onChange={() => setImportBehavior("copy")}
                />
                导入后保留原文件
              </label>
              <label>
                <input
                  type="radio"
                  checked={importBehavior === "move"}
                  onChange={() => setImportBehavior("move")}
                />
                导入后删除原文件
              </label>
              <span>真实文件不会被前端直接移动，后端命令会负责安全边界。</span>
            </div>
            {importResults.length > 0 ? (
              <div className="operation-summary">
                {importResults.slice(0, 4).map((item) => (
                  <span key={`${item.sourcePath}-${item.status}`}>
                    {item.status}: {item.relativePath ?? item.fileName ?? item.sourcePath}
                  </span>
                ))}
              </div>
            ) : null}
            {extractResult ? (
              <div className="operation-summary">
                <span>
                  extract {extractResult.provider}/{extractResult.model}: {extractResult.status}
                  {extractResult.isMock ? " fallback" : ""}
                </span>
                {extractResult.error ? <span>{extractResult.error}</span> : null}
              </div>
            ) : null}
            {workerRunResult ? (
              <div className="operation-summary">
                <span>
                  worker {workerRunResult.status}: processed {workerRunResult.processed}, moved {workerRunResult.moved}, failed {workerRunResult.failed}, conflicts {workerRunResult.conflicts}
                </span>
                {workerRunResult.items.slice(0, 3).map((item) => {
                  const metadataSummary = decisionMetadataSummary(item.decisionMetadata);
                  const reasonLabels = item.decisionMetadata?.confirmationReasons.map(confirmationReasonLabel) ?? [];
                  return (
                    <span key={item.queueId}>
                      #{item.queueId} {item.status}: {item.relativePath} {item.message}
                      {metadataSummary ? ` / ${metadataSummary}` : ""}
                      {reasonLabels.length > 0 ? ` / 确认原因：${reasonLabels.join(" / ")}` : ""}
                    </span>
                  );
                })}
              </div>
            ) : null}
            {listenerStatus ? (
              <div className="operation-summary">
                <span>
                  listener {listenerStatus.status}: {listenerStatus.watchPath ?? "未绑定路径"}
                </span>
                <span>
                  最近事件 {listenerStatus.lastEvent ?? "无"} · 入队 {listenerStatus.lastEnqueuedCount}
                </span>
                {listenerStatus.isFallback || listenerStatus.lastError ? (
                  <span>{listenerStatus.fallbackReason ?? listenerStatus.lastError}</span>
                ) : null}
              </div>
            ) : null}

            <section className="concept-card table-card">
              <div className="card-heading">
                <h3>
                  <ClipboardCheck size={18} aria-hidden="true" />
                  AI 整理队列
                </h3>
                <span>
                  {queueRows.length} · worker {workerStatus?.listener.status ?? "未连接"}
                </span>
              </div>
              <div className="queue-table">
                <div className="queue-head">
                  <span>文件</span>
                  <span>提取状态</span>
                  <span>AI 分类</span>
                  <span>目标路径</span>
                  <span>置信度</span>
                </div>
                {queueRows.map((item, index) => {
                  const isInboxItem = "relativePath" in item;
                  const sourcePath = isInboxItem ? item.relativePath : item.sourceRelativePath;
                  const targetPath = isInboxItem
                    ? `100-Organized / ${item.name}`
                    : item.targetRelativePath;
                  const confidence = isInboxItem ? 0 : item.confidence;
                  const isSelectedPlan = !isInboxItem && inboxPlanResult?.plan.sourceRelativePath === sourcePath;
                  return (
                    <button
                      type="button"
                      className={selectedInboxPath === sourcePath ? "queue-row active" : "queue-row"}
                      key={sourcePath}
                      onClick={() => {
                        setSelectedInboxPath(sourcePath);
                        setTargetMovePath(targetPath.replaceAll(" / ", "/"));
                      }}
                    >
                      <span>{fileIcon(sourcePath)} {sourcePath.split("/").pop()}</span>
                      <span className={isSelectedPlan ? "green-text" : "muted-text"}>
                        {isSelectedPlan ? `${inboxPlanResult?.extraction.status} / ${inboxPlanResult?.plan.status}` : "等待计划"}
                      </span>
                      <span>{isInboxItem ? "待抽取" : item.tags.join("、") || "待确认"}</span>
                      <span>{targetPath}</span>
                      <span>
                        {percentage(confidence)}
                        <i style={{ width: `${Math.max(18, confidence * 100)}%` }} />
                      </span>
                    </button>
                  );
                })}
                {queueRows.length === 0 ? <p className="empty-state">收集箱暂无待整理文件</p> : null}
              </div>
            </section>

            <section className="concept-card move-card">
              <div className="card-heading">
                <h3>
                  <Archive size={18} aria-hidden="true" />
                  移动与回滚
                </h3>
                {organizeResult ? <span>{organizeResult.message}</span> : null}
              </div>
              <div className="move-grid">
                <input
                  aria-label="收集箱文件"
                  value={selectedInboxPath}
                  onChange={(event) => setSelectedInboxPath(event.target.value)}
                  placeholder="000-收集箱/文件.md"
                />
                <input
                  aria-label="目标相对路径"
                  value={targetMovePath}
                  onChange={(event) => setTargetMovePath(event.target.value)}
                  placeholder="100-学校/课程资料/文件.md"
                />
                <button type="button" onClick={moveInboxItem} disabled={!vaultPath}>
                  <ArrowRight size={16} aria-hidden="true" />
                  移动
                </button>
                <button type="button" className="secondary-button" onClick={rollbackMove} disabled={!vaultPath}>
                  <RotateCcw size={16} aria-hidden="true" />
                  回滚
                </button>
              </div>
              {inboxPlanResult ? (() => {
                const plan = inboxPlanResult.plan;
                const metadataSummary = decisionMetadataSummary(plan);
                const reasonLabels = plan.confirmationReasons.map(confirmationReasonLabel);
                return (
                  <div className="plan-detail">
                    <span>{plan.provider}/{plan.model}</span>
                    <strong>{percentage(plan.confidence)}</strong>
                    <p>{plan.reason || plan.error || plan.summary}</p>
                    {metadataSummary ? <small>{metadataSummary}</small> : null}
                    {reasonLabels.length > 0 ? <small>确认原因：{reasonLabels.join(" / ")}</small> : null}
                    {plan.newDirectoryReason ? <small>新目录理由：{plan.newDirectoryReason}</small> : null}
                  </div>
                );
              })() : null}
            </section>
            {renderMovementLogPanel()}
            {renderAuditTimelinePanel()}
          </div>

          <aside className="inbox-aside">
            <section className="concept-card archive-map-card">
              <div className="card-heading">
                <h3>
                  <Archive size={18} aria-hidden="true" />
                  Archive Map
                </h3>
                <button type="button" className="text-link-button" onClick={rebuildArchiveMap} disabled={!vaultPath}>
                  重建
                </button>
              </div>
              <div className="archive-map-summary">
                <div><strong>{archiveMap?.directoryCount ?? 0}</strong><span>正式目录</span></div>
                <div><strong>{archiveMap?.fileCount ?? 0}</strong><span>样本文件</span></div>
                <div><strong>{archiveMap?.historyCount ?? 0}</strong><span>历史引用</span></div>
              </div>
              {archiveMap ? (
                <div className={archiveMap.health.isStale ? "archive-map-health is-stale" : "archive-map-health"}>
                  <strong>{archiveMapHealthLabel(archiveMap.health.status)}</strong>
                  <span>
                    缓存 {archiveMap.health.cachedDirectoryCount} · 当前 {archiveMap.health.currentDirectoryCount} · stale {archiveMap.health.staleDirectoryCount} · 锁定 {archiveMap.lockedDirectoryCount}
                  </span>
                  {archiveMap.health.staleReasons.length > 0 ? (
                    <small>{archiveMap.health.staleReasons.slice(0, 2).join(" · ")}</small>
                  ) : (
                    <small>地图与当前正式 Vault 目录一致</small>
                  )}
                </div>
              ) : null}
              <div className="archive-map-list">
                {(archiveMap?.directories ?? []).slice(0, 5).map((directory) => {
                  const hitStat = archiveMap?.topHitDirectories.find((item) => item.relativePath === directory.relativePath);
                  const draft = archiveRuleDraft(directory);
                  const targetName = selectedInboxPath.split("/").pop() || "新文件.md";
                  return (
                    <article
                      key={directory.relativePath}
                      className={draft.locked ? "is-locked" : undefined}
                    >
                      <div className="archive-map-directory-row">
                        <div>
                          <span>{directory.relativePath}</span>
                          {directory.semanticSummary ? (
                            <small className="archive-map-semantic-summary">
                              {directory.semanticSummary}
                            </small>
                          ) : null}
                          <small>
                            {directory.fileCount} files · 历史引用 {hitStat?.moveCount ?? directory.historicalMoves} · {directory.keywordHints.slice(0, 3).join(" / ") || "no hints"}
                          </small>
                          {(directory.headingHints ?? []).length > 0 ? (
                            <small>
                              标题 {(directory.headingHints ?? []).slice(0, 2).join(" / ")}
                            </small>
                          ) : null}
                        </div>
                        <button
                          type="button"
                          className="icon-soft-button"
                          onClick={() => setTargetMovePath(`${directory.relativePath}/${targetName}`)}
                          title="设为移动目标"
                        >
                          <ArrowRight size={15} aria-hidden="true" />
                        </button>
                      </div>
                      <div className="archive-map-rule-form">
                        <label className="archive-map-lock-toggle">
                          <input
                            type="checkbox"
                            checked={draft.locked}
                            onChange={(event) =>
                              updateArchiveRuleDraft(directory, { locked: event.currentTarget.checked })
                            }
                          />
                          <span>锁定目录规则</span>
                        </label>
                        <input
                          type="text"
                          value={draft.userNote}
                          placeholder="目录说明"
                          onChange={(event) =>
                            updateArchiveRuleDraft(directory, { userNote: event.currentTarget.value })
                          }
                        />
                        <textarea
                          value={draft.organizingHint}
                          placeholder="整理提示"
                          rows={2}
                          onChange={(event) =>
                            updateArchiveRuleDraft(directory, { organizingHint: event.currentTarget.value })
                          }
                        />
                        <button
                          type="button"
                          className="secondary-button full-width"
                          onClick={() => void saveArchiveRule(directory)}
                          disabled={!vaultPath || archiveRuleSavingPath === directory.relativePath}
                        >
                          {archiveRuleSavingPath === directory.relativePath ? "保存中" : "保存规则"}
                        </button>
                      </div>
                    </article>
                  );
                })}
                {archiveMap && archiveMap.directories.length === 0 ? (
                  <p className="empty-state">还没有正式归档目录</p>
                ) : null}
                {!archiveMap ? <p className="empty-state">选择并初始化 Vault 后生成归档地图</p> : null}
              </div>
              {archiveMap?.topHitDirectories.length ? (
                <div className="archive-map-hit-list">
                  <span>历史归档命中</span>
                  {archiveMap.topHitDirectories.slice(0, 3).map((directory) => (
                    <small key={directory.relativePath}>
                      {directory.relativePath} · {directory.moveCount} 次{directory.existsInCurrentMap ? "" : " · 不在当前地图"}
                    </small>
                  ))}
                </div>
              ) : null}
              {archiveMap?.staleDirectories.length ? (
                <div className="archive-map-stale-list">
                  <span>需关注目录</span>
                  {archiveMap.staleDirectories.slice(0, 3).map((directory) => (
                    <small key={`${directory.relativePath}-${directory.reason}`}>
                      {directory.relativePath} · {directory.reason}
                    </small>
                  ))}
                </div>
              ) : null}
              <small>{archiveMap?.markdownPath ?? ".thebrain/rules/archive-map.md"}</small>
              <small>{archiveMap?.rulesMarkdownPath ?? ".thebrain/rules/archive-map-rules.md"}</small>
              {archiveMap?.isFallback ? <small>{archiveMap.fallbackReason}</small> : null}
            </section>

            <section className="concept-card">
              <div className="card-heading">
                <h3>整理模板</h3>
                <button type="button" className="text-link-button">管理模板</button>
              </div>
              <div className="template-tabs">
                {["学生模式", "工作模式", "混合模式", "频率模式"].map((template) => (
                  <button
                    type="button"
                    key={template}
                    className={settings.organizationTemplate.includes(template.slice(0, 2)) ? "active" : undefined}
                    onClick={() => setSettings((current) => ({ ...current, organizationTemplate: template }))}
                  >
                    {template}
                  </button>
                ))}
              </div>
              <ul className="check-list">
                <li>优先识别课程与学习内容</li>
                <li>自动提取学习笔记与知识点</li>
                <li>识别作业、试题与参考资料</li>
              </ul>
            </section>

            <section className="concept-card">
              <div className="card-heading">
                <h3>冲突问题</h3>
                <span>{conflicts.length}</span>
              </div>
              {conflicts.slice(0, 3).map((conflict) => (
                <article className="schedule-card" key={conflict.id}>
                  <Shield size={20} aria-hidden="true" />
                  <div>
                    <strong>{conflict.sourceRelativePath || "unknown source"}</strong>
                    <span>{conflict.message}</span>
                    <small>目标：{conflict.targetRelativePath || "未给出"}</small>
                    {renderConflictDetails(conflict)}
                    {renderConflictControls(conflict)}
                    <textarea
                      aria-label="冲突规则答案"
                      value={conflictAnswers[conflict.id] ?? ""}
                      onChange={(event) =>
                        setConflictAnswers((answers) => ({
                          ...answers,
                          [conflict.id]: event.target.value,
                        }))
                      }
                      placeholder="这类冲突以后应该如何处理"
                      rows={3}
                    />
                  </div>
                  {conflict.recommendations?.[0] ? (
                    <button
                      type="button"
                      onClick={() => applyRecommendedRule(conflict, conflict.recommendations![0].rule.id)}
                    >
                      应用推荐
                    </button>
                  ) : null}
                  <button type="button" className="secondary-button" onClick={() => recordConflictRule(conflict)}>
                    记录规则
                  </button>
                  <button type="button" className="ghost-button" onClick={() => resolveConflict(conflict, selectedConflictAction(conflict))}>
                    {selectedConflictAction(conflict) === "skip" ? "跳过" : "提交选择"}
                  </button>
                </article>
              ))}
              {conflicts.length === 0 ? <p className="empty-state">暂无待处理冲突</p> : null}
            </section>

            {renderConflictRulePanel()}

            <section className="concept-card">
              <div className="card-heading">
                <h3>待确认 TODO / 日程</h3>
                <span>{pendingCandidates.length}</span>
              </div>
              {pendingCandidates.slice(0, 2).map((candidate) => (
                <article className="schedule-card" key={candidate.id}>
                  {candidate.kind === "schedule" ? (
                    <Calendar size={20} aria-hidden="true" />
                  ) : (
                    <ListChecks size={20} aria-hidden="true" />
                  )}
                  <div>
                    <strong>{candidate.title}</strong>
                    <span>{candidate.excerpt}</span>
                    <small>来源：{candidate.sourceRelativePath}</small>
                  </div>
                  <button type="button" onClick={() => confirmCandidate(candidate)}>
                    {candidate.kind === "schedule" ? "加入日程" : "确认为 TODO"}
                  </button>
                  <button type="button" className="secondary-button" onClick={() => dismissCandidate(candidate)}>忽略</button>
                </article>
              ))}
              {pendingCandidates.length === 0 ? <p className="empty-state">暂无待确认 TODO/日程候选</p> : null}
            </section>

            <section className="concept-card">
              <div className="card-heading">
                <h3>整理动态</h3>
                <button type="button" className="text-link-button">查看全部</button>
              </div>
              <div className="activity-list">
                {(organizePlan?.candidates ?? []).slice(0, 5).map((candidate, index) => (
                  <div key={candidate.id}>
                    <small>10:{21 - index}</small>
                    <span>{candidate.sourceRelativePath} {"->"} {candidate.targetRelativePath}</span>
                    <strong>{index === 0 ? "已完成" : "等待中"}</strong>
                  </div>
                ))}
                {!organizePlan ? (
                  <>
                    <div><small>10:21</small><span>{"课程笔记.pdf -> 100-学校 / 课程资料"}</span><strong>已完成</strong></div>
                    <div><small>10:20</small><span>{"客户会议录音.mp3 -> 处理中"}</span><strong>处理中</strong></div>
                  </>
                ) : null}
              </div>
            </section>
          </aside>
        </section>
      </div>
    );
  }

  function renderMarkdownWorkspace() {
    return (
      <div className="markdown-page">
        <div className="breadcrumb-row">
          <span>Vault</span>
          <ChevronRight size={14} aria-hidden="true" />
          <span>{relativePath.split("/").slice(0, -1).join(" / ") || "000-收集箱"}</span>
          <ChevronRight size={14} aria-hidden="true" />
          <strong>{relativePath.split("/").pop()}</strong>
          <div className="breadcrumb-actions">
            <span className="save-state"><Check size={14} aria-hidden="true" /> 已保存 09:42</span>
            <button type="button" className="ghost-button" onClick={exportMarkdown}>
              <Download size={15} aria-hidden="true" />
              导出
            </button>
            <button type="button" className="ghost-button">
              <Link2 size={15} aria-hidden="true" />
              插入链接
            </button>
          </div>
        </div>

        <section className="markdown-layout">
          <aside className="markdown-tree">
            <h3>Vault 文件</h3>
            <div className="tree-scroll large">{renderVaultTree(tree)}</div>
          </aside>

          <section className="editor-surface">
            <div className="editor-toolbar">
              {["正文", "H1", "H2", "H3", "B", "I"].map((tool) => (
                <button type="button" className="toolbar-button" key={tool}>{tool}</button>
              ))}
              <button type="button" className="toolbar-button"><Code2 size={15} aria-hidden="true" /></button>
              <button type="button" className="toolbar-button"><Link2 size={15} aria-hidden="true" /></button>
              <button type="button" className="toolbar-button"><ImageIcon size={15} aria-hidden="true" /></button>
              <button type="button" className="toolbar-button"><MoreHorizontal size={15} aria-hidden="true" /></button>
              <button type="button" className="save-inline" onClick={saveMarkdown}>
                <Save size={15} aria-hidden="true" />
                保存
              </button>
            </div>
            <div className="editor-preview-grid">
              <div className="code-editor-pane">
                <textarea
                  value={content}
                  onChange={(event) => setContent(event.target.value)}
                  aria-label="Markdown 正文"
                  spellCheck={false}
                />
                <footer>
                  <span>行 1，列 1</span>
                  <span>字数：{content.length}</span>
                  <span>Markdown</span>
                </footer>
              </div>
              <div className="preview-pane">
                <div className="pane-heading">
                  <h3>预览</h3>
                  <Eye size={15} aria-hidden="true" />
                </div>
                <article className="markdown-preview" dangerouslySetInnerHTML={{ __html: previewHtml }} />
              </div>
            </div>
          </section>

          <aside className="markdown-inspector">
            <section className="inspector-card">
              <h3>AI 助手</h3>
              <button type="button"><Bot size={15} aria-hidden="true" /> 总结本章</button>
              <button type="button"><Sparkles size={15} aria-hidden="true" /> 生成复习卡片</button>
              <button type="button"><MessageSquare size={15} aria-hidden="true" /> 解释算法复杂度</button>
              <div className="mini-prompt">
                <input placeholder="询问关于本文的问题..." />
                <button type="button" className="send-button"><Send size={15} aria-hidden="true" /></button>
              </div>
            </section>

            <section className="inspector-card">
              <h3>相关文件</h3>
              {relatedFiles.map((file) => (
                <button type="button" key={file.relativePath} onClick={() => openMarkdown(file.relativePath)}>
                  {fileIcon(file.relativePath)}
                  <span>{file.name}</span>
                  <small>{file.relativePath.split("/").slice(0, -1).join(" / ")}</small>
                </button>
              ))}
              {relatedFiles.length === 0 ? <p className="empty-state">暂无相关文件</p> : null}
            </section>

            <section className="inspector-card">
              <h3>文档信息</h3>
              <dl className="doc-info">
                <dt>创建日期</dt>
                <dd>2025/05/10 14:32</dd>
                <dt>最近修改</dt>
                <dd>今天 09:42</dd>
                <dt>类型</dt>
                <dd>学习笔记</dd>
                <dt>访问频率</dt>
                <dd><span className="frequency-bars"><i /><i /><i /><i /><i /></span></dd>
              </dl>
              <textarea
                className="frontmatter-editor"
                value={frontmatterText}
                onChange={(event) => setFrontmatterText(event.target.value)}
                aria-label="Frontmatter JSON"
              />
              {exported ? <textarea className="export-output" readOnly value={exported} aria-label="导出 Markdown" /> : null}
            </section>
          </aside>
        </section>
      </div>
    );
  }

  function renderStickyNotes() {
    return (
      <div className="sticky-page">
        <section className="sticky-window" aria-label="便利贴窗口">
          <header>
            <div className="sticky-tabs">
              <button type="button" className={stickyMode === "new" ? "active" : undefined} onClick={() => setStickyMode("new")}>新建</button>
              <button type="button" className={stickyMode === "continue" ? "active" : undefined} onClick={() => setStickyMode("continue")}>
                继续之前的便签
                <ChevronDown size={14} aria-hidden="true" />
              </button>
            </div>
            <div className="sticky-window-actions">
              <button type="button" className="icon-soft-button" title="置顶" onClick={() => activeNote && updateActiveNote({ pinned: !activeNote.pinned })}>
                <Pin size={17} aria-hidden="true" />
              </button>
              <button type="button" className="icon-soft-button" title="关闭" onClick={() => setActiveView("dashboard")}>
                <X size={18} aria-hidden="true" />
              </button>
            </div>
          </header>

          {stickyMode === "continue" ? (
            <div className="sticky-history-popover">
              {stickyNotes.slice(0, 3).map((note) => (
                <button
                  type="button"
                  key={note.id}
                  onClick={() => {
                    setActiveNoteId(note.id);
                    setStickyMode("new");
                  }}
                >
                  <FileText size={18} aria-hidden="true" />
                  <span>{note.title || "未命名便签"}</span>
                  <small>{formatDate(note.updatedAt)}</small>
                </button>
              ))}
            </div>
          ) : null}

          {activeNote ? (
            <>
              <input
                className="sticky-title-input"
                aria-label="便利贴标题"
                value={activeNote.title}
                placeholder="标题（可选）"
                onChange={(event) => updateActiveNote({ title: event.target.value })}
              />
              <textarea
                className="sticky-body-input"
                value={activeNote.content}
                placeholder="写点什么......"
                onChange={(event) => updateActiveNote({ content: event.target.value })}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && event.ctrlKey) {
                    event.preventDefault();
                    void persistStickyNote(activeNote);
                  }
                }}
              />
              <div className="sticky-tool-row">
                <button type="button" title="Markdown"><Code2 size={17} aria-hidden="true" /></button>
                <button type="button" title="截图"><ImageIcon size={17} aria-hidden="true" /></button>
                <button type="button" title="附件"><Paperclip size={17} aria-hidden="true" /></button>
                <button type="button" title="链接"><Link2 size={17} aria-hidden="true" /></button>
              </div>
              <footer>
                <span>
                  <Folder size={17} aria-hidden="true" />
                  已自动保存到 000-收集箱 / 便签草稿
                  <Circle size={8} aria-hidden="true" />
                </span>
                <kbd>Ctrl+Space 打开</kbd>
                <kbd>Ctrl+Enter 新建便签</kbd>
                <button type="button" onClick={() => persistStickyNote(activeNote)}>
                  <Save size={16} aria-hidden="true" />
                  保存
                </button>
              </footer>
            </>
          ) : (
            <div className="empty-sticky">
              <p>暂无便利贴</p>
              <button type="button" onClick={addStickyNote}>新建便签</button>
            </div>
          )}
        </section>

        <aside className="sticky-side-list">
          <div className="card-heading">
            <h3>最近便签</h3>
            <button type="button" className="tiny-icon-button" onClick={addStickyNote}>
              <Plus size={14} aria-hidden="true" />
            </button>
          </div>
          {stickyNotes.map((note) => (
            <button
              type="button"
              key={note.id}
              className={activeNote?.id === note.id ? "active" : undefined}
              onClick={() => setActiveNoteId(note.id)}
            >
              <StickyNote size={15} aria-hidden="true" />
              <span>{note.title || "未命名便签"}</span>
              <small>{formatDate(note.updatedAt)}</small>
            </button>
          ))}
          {activeNote ? (
            <button type="button" className="danger-soft" onClick={deleteActiveNote}>
              <Trash2 size={15} aria-hidden="true" />
              删除当前便签
            </button>
          ) : null}
        </aside>
      </div>
    );
  }

  function renderPersonalPage() {
    // Personal workspace is partially data-backed by get_workspace_insights.
    // Learning progress and review recommendations remain future/placeholder surfaces.
    return (
      <div className="personal-page">
        {insightsFallbackText ? <p className="insights-note">{insightsFallbackText}</p> : null}
        <section className="concept-grid three">
          <article className="concept-card chart-card">
            <div className="card-heading">
              <h3><FileText size={18} aria-hidden="true" /> Vault 统计</h3>
              <button type="button" className="chip-button">{realWorkspaceInsights ? "真实统计" : "树派生"}</button>
            </div>
            <div className="stat-pair">
              <div><span>Vault 文件</span><strong>{vaultFileCount}</strong></div>
              <div><span>Markdown</span><strong>{vaultMarkdownCount}</strong></div>
            </div>
            <div className="line-chart" aria-label="文件活跃度趋势">
              <i style={{ height: "35%" }} /><i style={{ height: "62%" }} /><i style={{ height: "48%" }} /><i style={{ height: "76%" }} /><i style={{ height: "58%" }} /><i style={{ height: "82%" }} /><i style={{ height: "64%" }} />
            </div>
            <p className="insights-note muted">趋势图仍为后续时间序列占位。</p>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><FileText size={18} aria-hidden="true" /> 最近文件</h3>
              <button type="button" className="chip-button">{realWorkspaceInsights ? "Vault/SQLite" : "树派生"}</button>
            </div>
            <div className="rank-list">
              {recentWorkspaceFiles.map((file, index) => (
                <button type="button" key={file.relativePath} onClick={() => isMarkdownPath(file.relativePath) ? openMarkdown(file.relativePath) : setRelativePath(file.relativePath)}>
                  <strong>{index + 1}</strong>
                  <span>{file.name}</span>
                  <small>{file.modifiedAt ? formatDate(file.modifiedAt) : formatBytes(file.sizeBytes)}</small>
                  <i style={{ width: `${75 - index * 8}%` }} />
                </button>
              ))}
              {recentWorkspaceFiles.length === 0 ? <p className="empty-state">暂无最近文件数据</p> : null}
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><Target size={18} aria-hidden="true" /> TODO / 日程 / 便签</h3>
            </div>
            <div className="goal-grid">
              <div><CheckCircle2 size={26} aria-hidden="true" /><strong>{openTodoCount}</strong><span>正式 TODO</span></div>
              <div><Calendar size={26} aria-hidden="true" /><strong>{scheduledActionCount}</strong><span>正式日程</span></div>
              <div><StickyNote size={26} aria-hidden="true" /><strong>{stickyActiveCount}</strong><span>活跃便签</span></div>
              <div><Star size={26} aria-hidden="true" /><strong>{pendingTodoCount + pendingScheduleCount}</strong><span>待确认候选</span></div>
            </div>
          </article>
        </section>

        <section className="concept-grid three">
          <article className="concept-card">
            <div className="card-heading">
              <h3><Activity size={18} aria-hidden="true" /> 学习进度</h3>
              <button type="button" className="chip-button">后续/占位</button>
            </div>
            {["期末复习进度", "《深度学习》阅读进度", "CS61A 课程学习"].map((item, index) => (
              <div className="progress-line" key={item}>
                <span>{item}</span>
                <strong>{68 - index * 8}%</strong>
                <i><b style={{ width: `${68 - index * 8}%` }} /></i>
              </div>
            ))}
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><Shield size={18} aria-hidden="true" /> 推荐复习</h3>
              <button type="button" className="chip-button">后续/占位</button>
            </div>
            <div className="review-table">
              {["线性代数基础知识", "深度学习-损失函数", "Python 装饰器详解", "计算机网络-TCP 三次握手"].map((item, index) => (
                <div key={item}>
                  <span>{item}</span>
                  <small>{2 + index} 天后</small>
                </div>
              ))}
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><Database size={18} aria-hidden="true" /> RAG 与候选积累</h3>
              <button type="button" className="chip-button">{realWorkspaceInsights ? "真实统计" : "状态派生"}</button>
            </div>
            <div className="accumulation-list">
              <div><FileText size={22} aria-hidden="true" /><span>RAG 文档</span><strong>{ragDocumentCount}</strong></div>
              <div><MessageCircle size={22} aria-hidden="true" /><span>RAG 会话</span><strong>{ragConversationCount}</strong></div>
              <div><Bot size={22} aria-hidden="true" /><span>正式行动项</span><strong>{formalActionCount}</strong></div>
              <div><CheckCircle2 size={22} aria-hidden="true" /><span>已完成行动</span><strong>{completedActionCount}</strong></div>
            </div>
          </article>
        </section>

        <section className="stats-ribbon">
          <div><Activity size={29} aria-hidden="true" /><strong>{vaultFileCount}</strong><span>Vault 文件</span></div>
          <div><FileText size={29} aria-hidden="true" /><strong>{vaultMarkdownCount}</strong><span>Markdown</span></div>
          <div><CheckCircle2 size={29} aria-hidden="true" /><strong>{recentAuditCount}</strong><span>近期 audit</span></div>
          <div><Archive size={29} aria-hidden="true" /><strong>{movementCompletedCount}</strong><span>已完成移动</span></div>
          <div><Shield size={29} aria-hidden="true" /><strong>{movementIssueCount}</strong><span>冲突/失败</span></div>
        </section>
        <p className="insights-note">
          最新 audit：{latestAuditEventType ? `${latestAuditEventType} / ${formatDate(latestAuditAt)}` : "暂无记录"}。
        </p>
      </div>
    );
  }

  function renderProjectWorkspace() {
    const projectFileCount = selectedProject?.fileCount ?? 0;
    const projectMarkdownCount = selectedProject?.markdownCount ?? 0;
    const projectProgress =
      projectFileCount > 0 ? Math.round((projectMarkdownCount / projectFileCount) * 100) : 0;
    const projectSourceLabel =
      selectedProject?.source === "insights" ? "真实统计" : selectedProject?.source === "tree" ? "Vault 树派生" : "占位兜底";
    const projectScopedActionItems = activeActionItems
      .filter((item) => selectedProject?.relativePath && item.sourceRelativePath?.startsWith(selectedProject.relativePath))
      .slice(0, 4);
    const projectAuditRows = auditEvents.filter((event) => !event.isFallback).slice(0, 4);

    return (
      <div className="project-page">
        {insightsFallbackText || selectedProject?.source !== "insights" ? (
          <p className="insights-note">
            {insightsFallbackText || `项目统计使用${projectSourceLabel}，后端 projects insights 暂无可用数据。`}
          </p>
        ) : null}
        <div className="project-title-row">
          <div>
            <span>项目工作台</span>
            <h2>
              <FolderOpen size={28} aria-hidden="true" />
              {selectedProject?.name ?? "TheBrain"}
              <ChevronDown size={18} aria-hidden="true" />
            </h2>
            <p>{projectSourceLabel} · 文件 {projectFileCount} · Markdown {projectMarkdownCount} · 最近更新 {formatDate(selectedProject?.updatedAt)}</p>
          </div>
          <div className="project-actions">
            <button type="button" className="ghost-button" disabled><Settings size={16} aria-hidden="true" /> 设置后续</button>
            <button type="button" className="icon-soft-button"><MoreHorizontal size={16} aria-hidden="true" /></button>
          </div>
        </div>

        <section className="project-grid">
          <article className="concept-card">
            <div className="card-heading"><h3><ClipboardCheck size={18} aria-hidden="true" /> 项目概览</h3></div>
            <p>{selectedProject?.relativePath ?? "未选择项目"} · {formatBytes(selectedProject?.totalBytes)} · {projectSourceLabel}</p>
            <div className="milestone-row">
              <CheckCircle2 size={17} aria-hidden="true" />
              <span>{projectMarkdownCount} 个 Markdown / {projectFileCount} 个文件</span>
              <small>{formatDate(selectedProject?.updatedAt)}</small>
            </div>
            <div className="project-progress"><i style={{ width: `${projectProgress}%` }} /></div>
            <footer><span>Markdown 占比</span><strong>{projectProgress}%</strong></footer>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><MessageSquare size={18} aria-hidden="true" /> 最新 Audit / Agent 历史</h3>
              <button type="button" className="chip-button">后续/占位</button>
            </div>
            <div className="agent-history">
              {projectAuditRows.length > 0 ? projectAuditRows.map((event) => (
                <div key={event.id}>
                  <MessageCircle size={16} aria-hidden="true" />
                  <span>{event.eventType}</span>
                  <small>{formatDate(event.createdAt)}</small>
                </div>
              )) : latestAuditEventType ? (
                <div>
                  <MessageCircle size={16} aria-hidden="true" />
                  <span>{latestAuditEventType}</span>
                  <small>{formatDate(latestAuditAt)}</small>
                </div>
              ) : (
                <p className="empty-state">暂无真实 Agent history；后续接入项目级 Agent 事件。</p>
              )}
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><FileText size={18} aria-hidden="true" /> 相关文件</h3>
              <button type="button" className="chip-button">{selectedProject?.source === "insights" ? "recentFiles" : "树派生"}</button>
            </div>
            <div className="project-files">
              {projectRelatedFiles.length > 0 ? projectRelatedFiles.map((file) => (
                <button type="button" key={file.relativePath} onClick={() => isMarkdownPath(file.relativePath) ? openMarkdown(file.relativePath) : setRelativePath(file.relativePath)}>
                  {fileIcon(file.relativePath)}
                  <span>{file.name}</span>
                  <small>{file.modifiedAt ? formatDate(file.modifiedAt) : formatBytes(file.sizeBytes)}</small>
                </button>
              )) : <p className="empty-state">暂无来自当前项目的真实相关文件。</p>}
            </div>
            <button type="button" className="full-width ghost-button" onClick={() => setActiveView("inbox")}>
              <Plus size={16} aria-hidden="true" />
              添加文件
            </button>
          </article>

          <article className="concept-card project-chat">
            <div className="card-heading"><h3><Bot size={18} aria-hidden="true" /> 与 Agent 对话 / 下达指令</h3></div>
            {renderPromptBox("project")}
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><ListChecks size={18} aria-hidden="true" /> 下一步行动 / TODO</h3>
              <button type="button" className="chip-button">正式/SQLite</button>
            </div>
            <div className="project-todos">
              {projectScopedActionItems.map((item) => (
                <div className="project-todo-row" key={`${item.kind}-${item.id}`}>
                  <input
                    type="checkbox"
                    checked={isCompletedActionItem(item)}
                    onChange={() => completeActionItem(item)}
                    aria-label={item.title}
                  />
                  <span>{item.title}</span>
                  <small>{actionKindLabel(item)} · {actionDateLabel(item)}</small>
                </div>
              ))}
              {projectScopedActionItems.length === 0 ? (
                <p className="empty-state">暂无项目路径匹配的正式行动项；可从收集箱候选确认生成。</p>
              ) : null}
            </div>
          </article>
        </section>

        <section className="quick-actions">
          <span>快速操作：</span>
          <button type="button" onClick={() => setActiveView("markdown")}><Pencil size={15} aria-hidden="true" /> 打开 Markdown</button>
          <button type="button" onClick={() => setActiveView("inbox")}><Upload size={15} aria-hidden="true" /> 上传文件</button>
          <button type="button" disabled><CheckCircle2 size={15} aria-hidden="true" /> 任务后续</button>
          <button type="button" disabled><Link2 size={15} aria-hidden="true" /> GitHub 后续</button>
        </section>
      </div>
    );
  }

  function renderSettingsPage() {
    return (
      <div className="settings-page">
        <section className="settings-grid">
          <article className="settings-card">
            <h3><span>1</span> Vault 与本地存储</h3>
            <label>Vault 路径 <input value={vaultPath} onChange={(event) => setVaultPath(event.target.value)} placeholder="D:\\TheBrain\\Vault" /></label>
            <label>000-收集箱 <input value="000-收集箱" readOnly /></label>
            <div className="segmented">
              <button type="button" className={importBehavior === "copy" ? "active" : undefined} onClick={() => setImportBehavior("copy")}>保留原文件</button>
              <button type="button" className={importBehavior === "move" ? "active" : undefined} onClick={() => setImportBehavior("move")}>删除原文件</button>
            </div>
            <label>本地数据库位置 <input value=".thebrain/index.sqlite" readOnly /></label>
            <p>本地优先，所有数据先保存在本地。</p>
          </article>

          <article className="settings-card">
            <h3><span>2</span> AI 与 API Key</h3>
            <label>Provider <select value="MiMo / OpenAI 兼容" onChange={() => undefined}><option>MiMo / OpenAI 兼容</option></select></label>
            <label>API Key <input type="password" value={mimoKeyInput} onChange={(event) => setMimoKeyInput(event.target.value)} placeholder={mimoStatus?.hasKey ? "粘贴新 key 可覆盖当前安全文件" : "粘贴 MiMo API key"} aria-label="MiMo API Key" /></label>
            <div className="connection-row">
              <Circle size={8} aria-hidden="true" />
              {mimoSettingsSummary}
              <button type="button" className="ghost-button" onClick={() => vaultPath && refreshOperations(vaultPath)}>刷新</button>
            </div>
            {mimoStatus?.keyMessage ? <p className="field-hint">{mimoStatus.keyMessage}</p> : null}
            <label className="toggle-line"><input type="checkbox" checked readOnly /> 启用 AI 功能</label>
            <button type="button" className="ghost-button" onClick={saveMimoKey} disabled={!vaultPath || !mimoKeyInput.trim()}>保存 Key 到 Vault</button>
            <label>每月预算上限 <input type="number" value={settings.budgetMonthlyCents} onChange={(event) => setSettings((current) => ({ ...current, budgetMonthlyCents: Number(event.target.value) || 0 }))} /></label>
            <button type="button" className="ghost-button" onClick={saveBudget}>保存预算</button>
          </article>

          <article className="settings-card">
            <h3><span>3</span> 便利贴与快捷键</h3>
            <label>打开便利贴（全局） <input value="Ctrl + Space" readOnly /></label>
            <label>立即整理（全局） <input value="Ctrl + Enter" readOnly /></label>
            <label>便利贴自动保存 <select value={settings.autoSaveIntervalSeconds} onChange={(event) => setSettings((current) => ({ ...current, autoSaveIntervalSeconds: Number(event.target.value) || 3 }))}><option value={3}>输入停止 3 秒后</option><option value={20}>每 20 秒</option></select></label>
            <label className="toggle-line"><input type="checkbox" checked={settings.enableGlobalShortcut} onChange={(event) => setSettings((current) => ({ ...current, enableGlobalShortcut: event.target.checked }))} /> 窗口置顶</label>
            <label>新建便签位置 <select value="居中" onChange={() => undefined}><option>居中</option></select></label>
          </article>

          <article className="settings-card template-settings">
            <h3><span>4</span> 整理模板</h3>
            <p>选择默认整理模板，AI 将按所选模式进行整理。</p>
            <div className="template-card-grid">
              {["学生模式", "工作模式", "混合模式", "频率模式"].map((template) => (
                <button
                  type="button"
                  key={template}
                  className={settings.organizationTemplate === template ? "active" : undefined}
                  onClick={() => setSettings((current) => ({ ...current, organizationTemplate: template }))}
                >
                  <Wand2 size={26} aria-hidden="true" />
                  <strong>{template}</strong>
                  <span>{template === "学生模式" ? "适合学习与考试复习" : "保留扩展入口"}</span>
                </button>
              ))}
            </div>
            <label>默认模板 <select value={settings.organizationTemplate} onChange={(event) => setSettings((current) => ({ ...current, organizationTemplate: event.target.value }))}><option>学生模式</option><option>工作模式</option><option>混合模式</option><option>频率模式</option></select></label>
          </article>

          <article className="settings-card">
            <h3><span>5</span> Markdown 与编辑器</h3>
            <label>Frontmatter 处理 <select value="保留并更新" onChange={() => undefined}><option>保留并更新</option></select></label>
            <label>预览默认行为 <select value="右侧预览" onChange={() => undefined}><option>右侧预览</option></select></label>
            <label>字体大小 <input type="number" value={15} readOnly /></label>
            <label>行宽限制 <input type="number" value={880} readOnly /></label>
            <label>代码块主题 <select value="跟随系统" onChange={() => undefined}><option>跟随系统</option></select></label>
          </article>

          <article className="settings-card">
            <h3><span>6</span> 同步与备份</h3>
            <div className="local-first-note">TheBrain 采用本地优先策略，所有数据先保存在本地，可按需与云端或其他设备同步。</div>
            <label>上次同步时间 <input value="5 分钟前" readOnly /></label>
            <div className="connection-row"><Circle size={8} aria-hidden="true" /> 已同步 <button type="button" className="ghost-button">立即同步</button></div>
            <label>备份目录 <input value="D:\\TheBrain\\Backups" readOnly /></label>
            <label className="toggle-line"><input type="checkbox" checked readOnly /> 自动备份</label>
            <label>保留版本数 <select value={30} onChange={() => undefined}><option value={30}>30 个版本</option></select></label>
          </article>
        </section>
        <div className="settings-footer">
          <button type="button" className="ghost-button" onClick={() => setSettings(defaultAppSettings)}>取消</button>
          <button type="button" onClick={saveSettings}><Save size={16} aria-hidden="true" /> 保存设置</button>
        </div>
      </div>
    );
  }

  function renderDragOverlay() {
    if (!dragActive) return null;
    return (
      <div className="drag-overlay" role="dialog" aria-label="拖拽导入">
        <div className="drag-panel">
          <CloudUpload size={72} aria-hidden="true" />
          <h2>松开鼠标，导入到 000-收集箱</h2>
          <p>文件将默认复制到收集箱，随后由 AI 自动识别与整理</p>
          <div className="drag-choice-row">
            <label>
              <input type="radio" checked={importBehavior === "copy"} onChange={() => setImportBehavior("copy")} />
              复制原文件（默认）
            </label>
            <label>
              <input type="radio" checked={importBehavior === "move"} onChange={() => setImportBehavior("move")} />
              移动并删除原文件
            </label>
          </div>
          <div className="import-type-row overlay-types">
            {importTypes.map((type) => <span key={type.label}>{type.icon}{type.label}</span>)}
          </div>
          <div className="drop-hint"><RefreshCw size={15} aria-hidden="true" /> 你还可以拖入文本片段、截图、链接等内容</div>
        </div>
      </div>
    );
  }

  function renderActiveView() {
    if (activeView === "inbox") return renderInboxPage();
    if (activeView === "markdown") return renderMarkdownWorkspace();
    if (activeView === "notes") return renderStickyNotes();
    if (activeView === "personal") return renderPersonalPage();
    if (activeView === "project") return renderProjectWorkspace();
    if (activeView === "settings") return renderSettingsPage();
    return renderDashboard();
  }

  return (
    <main
      className="app-shell"
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {renderLeftSidebar()}
      <section className="workspace">
        {renderTopbar()}
        {appError ? (
          <section className="alert-panel" role="alert">
            <Shield size={18} aria-hidden="true" />
            <div>
              <strong>{appError.title}</strong>
              <span>{appError.detail}</span>
              <small>{appError.recovery}</small>
            </div>
          </section>
        ) : null}
        <div className="status-strip">
          <span>{vaultPath || "未选择 Vault"}</span>
          <span>{status}</span>
          <span>队列：{queueStateLabel(queueStatus?.state)} · 待处理 {queueStatus?.pending ?? 0}</span>
          <span>监听：{listenerStatus?.status ?? "未连接"} · 入队 {listenerStatus?.lastEnqueuedCount ?? 0}</span>
          <span>Worker：{workerStatus?.listener.status ?? "未连接"} · 冲突 {workerStatus?.conflicts ?? queueStatus?.conflicts ?? 0}</span>
          <span>预算：{budgetStatus ? formatBudget(budgetStatus.remainingCents) : "未连接"}</span>
          <span>MiMo：{mimoKeyLabel}</span>
          <span>RAG：{ragStatus ? `${ragStatus.documentCount} 文档 / ${ragStatus.chunkCount} chunks` : "未连接"}</span>
          <span>Insights：{workspaceInsights ? (insightsAreFallback ? "统计降级/占位" : "已连接") : "未连接"}</span>
          {initResult ? <span>Index 已初始化</span> : null}
          {usage?.isFallback ? <span>Usage fallback</span> : null}
        </div>
        <div className="workspace-scroll">{renderActiveView()}</div>
      </section>
      {renderDragOverlay()}
    </main>
  );
}
