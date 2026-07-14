/**
 * Tauri IPC 封装 + 事件订阅
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export class TauriError extends Error {
  constructor(
    message: string,
    public kind: string,
  ) {
    super(message);
    this.name = "TauriError";
  }
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e: any) {
    if (e && typeof e === "object" && "kind" in e && "message" in e) {
      throw new TauriError(e.message, e.kind);
    }
    throw new TauriError(String(e), "unknown");
  }
}

// ==================== 类型（与 Rust 端 models 对应） ====================

export type CommandType = "cmd" | "pwsh" | "powershell" | "python" | "node" | "bash" | "script";

/** UI 显示用的中文标签（按 PowerShell 类型加注释） */
export const COMMAND_TYPE_LABEL: Record<CommandType, string> = {
  cmd: "cmd",
  pwsh: "PowerShell 7+ (自动降级)",
  powershell: "PowerShell 5.1 (旧版)",
  python: "python",
  node: "node",
  bash: "bash",
  script: "script",
};

export type NodeStatus = "pending" | "running" | "success" | "failed" | "cancelled" | "timeout";
export type StreamKind = "stdout" | "stderr" | "system";

export interface Command {
  id: string;
  name: string;
  description: string | null;
  category: string | null;
  type: CommandType;
  current_ver: number;
  tags: string[];
  created_at: number;
  updated_at: number;
}

export interface CommandVersion {
  id: number;
  command_id: string;
  version: number;
  template: string;
  working_dir: string | null;
  env: Record<string, string> | null;
  timeout_ms: number | null;
  shell: string | null;
  note: string | null;
  created_at: number;
}

export interface Param {
  id: string;
  command_ver_id: number;
  name: string;
  label: string;
  type: ParamType;
  required: boolean;
  default_value: unknown;
  options: unknown;
  validation: unknown;
  sensitive: boolean;
  description: string | null;
  sort_order: number;
}

export type ParamType =
  | "text"
  | "textarea"
  | "number"
  | "boolean"
  | "select"
  | "multiselect"
  | "file"
  | "dir"
  | "password"
  | "json"
  | "datetime"
  | "cron";

export interface CommandDetail extends Command {
  version: CommandVersion;
  params: Param[];
}

export interface Workflow {
  id: string;
  name: string;
  description: string | null;
  enabled: boolean;
  trigger_type: string;
  created_at: number;
  updated_at: number;
}

export interface WorkflowNode {
  id: string;
  workflow_id: string;
  type: string;
  command_id: string | null;
  config: Record<string, unknown>;
  position_x: number;
  position_y: number;
  sort_order: number;
}

export interface WorkflowEdge {
  id: string;
  workflow_id: string;
  source_node: string;
  target_node: string;
  source_port: string | null;
  target_port: string | null;
  condition: unknown;
}

export interface WorkflowDetail extends Workflow {
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
}

export interface AppInfo {
  name: string;
  version: string;
  started_at: number;
  uptime_seconds: number;
}

export interface DagValidation {
  ok: boolean;
  errors: string[];
  warnings: string[];
  topo_order: string[];
  parallel_layers: string[][];
}

export interface CommandPreview {
  rendered: string;
  env: Record<string, string>;
  cwd: string | null;
  timeout_ms: number | null;
  blacklisted: string | null;
  dangerous: boolean;
  warnings: string[];
}

export interface RunCommandResponse {
  execution_id: string;
  status: NodeStatus;
  exit_code: number | null;
  duration_ms: number;
  stdout: string;
  stderr: string;
  error: string | null;
}

export interface ExecutionSummary {
  id: string;
  command_id: string;
  command_name: string;
  status: NodeStatus;
  trigger: string;
  started_at: number | null;
  finished_at: number | null;
  duration_ms: number | null;
  error: string | null;
}

export interface ExecutionDetail extends ExecutionSummary {
  input_params: unknown;
  stdout: string | null;
  stderr: string | null;
}

// ==================== 事件 ====================

export type RunEvent =
  | {
      kind: "execution_started";
      execution_id: string;
      command_id: string;
      command_name: string;
      started_at: number;
    }
  | {
      kind: "node_started";
      execution_id: string;
      node_id: string;
      node_name: string;
      started_at: number;
    }
  | {
      kind: "node_log";
      execution_id: string;
      node_id: string;
      stream: StreamKind;
      content: string;
      ts: number;
    }
  | {
      kind: "node_finished";
      execution_id: string;
      node_id: string;
      exit_code: number | null;
      status: NodeStatus;
      duration_ms: number;
    }
  | {
      kind: "execution_finished";
      execution_id: string;
      status: NodeStatus;
      duration_ms: number;
      error: string | null;
    }
  | { kind: "execution_cancelled"; execution_id: string };

export const RUN_EVENT = "run-event";

export function onRunEvent(handler: (e: RunEvent) => void): Promise<UnlistenFn> {
  return listen<RunEvent>(RUN_EVENT, (e) => handler(e.payload));
}

// ==================== Schedule & History ====================

export interface ScheduleRecord {
  id: string;
  workflow_id: string;
  cron_expr: string;
  timezone: string;
  enabled: boolean;
  last_run_at: number | null;
  next_run_at: number | null;
  mode: string;
  os_task_id: string | null;
  created_at: number;
}

export interface ScheduleWithWorkflow extends ScheduleRecord {
  workflow_name: string;
  workflow_enabled: boolean;
}

export interface CreateScheduleInput {
  workflow_id: string;
  cron_expr: string;
  timezone?: string;
  enabled?: boolean;
  mode?: string;
}

export interface UpdateScheduleInput {
  id: string;
  cron_expr?: string;
  timezone?: string;
  enabled?: boolean;
  mode?: string;
}

export interface HistorySummary {
  id: string;
  workflow_id: string;
  workflow_name: string;
  trigger: string;
  status: string;
  started_at: number | null;
  finished_at: number | null;
  duration_ms: number | null;
  error: string | null;
}

export interface NodeRunInfo {
  id: string;
  node_id: string;
  status: string;
  started_at: number | null;
  finished_at: number | null;
  duration_ms: number | null;
  exit_code: number | null;
  stdout: string | null;
  stderr: string | null;
  output_data: unknown;
  error: string | null;
}

export interface HistoryDetail extends HistorySummary {
  input_params: unknown;
  rendered_template: string | null;
  node_runs: NodeRunInfo[];
}

export interface HistoryStats {
  total: number;
  success: number;
  failed: number;
  running: number;
  avg_duration_ms: number;
  last_24h_count: number;
  by_status: Array<{ status: string; count: number }>;
}

export interface ClearHistoryInput {
  status?: string;
  workflow_id?: string;
  before_ts?: number;
  dry_run?: boolean;
}

export interface ClearHistoryResult {
  deleted: number;
}

// ==================== 分类树 (Phase 7) ====================

export interface Category {
  id: string;
  parent_id: string | null;
  name: string;
  icon: string | null;
  sort_order: number;
  created_at: number;
}

export interface CategoryNode {
  id: string;
  parent_id: string | null;
  name: string;
  icon: string | null;
  sort_order: number;
  subcategories: CategoryNode[];
  commands: Command[];
}

export interface CreateCategoryInput {
  name: string;
  parent_id?: string;
  icon?: string;
}

export interface MoveCategoryInput {
  id: string;
  new_parent_id?: string;
}

export interface MoveCommandInput {
  command_id: string;
  category_id?: string;
}

// ==================== 收藏 (Phase 7) ====================

export interface Favorite {
  command_id: string;
  sort_order: number;
  created_at: number;
  command: Command | null;
}

export interface ReorderFavoritesInput {
  command_ids: string[];
}

// ==================== 插件 (Phase 7) ====================

export interface Plugin {
  id: string;
  name: string;
  version: string;
  author: string | null;
  description: string | null;
  format: string;
  entry: string;
  manifest: string;
  enabled: boolean;
  installed_at: number;
  updated_at: number;
}

export interface PluginInfo {
  plugin: Plugin;
  has_manifest: boolean;
  size_bytes: number;
}

// ==================== YAML (Phase 8) ====================

export interface YamlImportResult {
  workflow_id: string;
  name: string;
}

// ==================== API ====================

export const api = {
  system: {
    getAppInfo: () => call<AppInfo>("get_app_info"),
  },
  library: {
    list: (params?: { category?: string; search?: string }) =>
      call<Command[]>("list_commands", params),
    get: (id: string) => call<CommandDetail>("get_command", { id }),
    create: (input: CreateCommandInput) => call<string>("create_command", { input }),
    update: (input: UpdateCommandInput) => call<void>("update_command", { input }),
    remove: (id: string) => call<void>("delete_command", { id }),
  },
  workflow: {
    list: () => call<Workflow[]>("list_workflows"),
    get: (id: string) => call<WorkflowDetail>("get_workflow", { id }),
    create: (input: CreateWorkflowInput) => call<string>("create_workflow", { input }),
    update: (input: UpdateWorkflowInput) => call<void>("update_workflow", { input }),
    remove: (id: string) => call<void>("delete_workflow", { id }),
    validateDag: (input: { nodes: WorkflowNode[]; edges: WorkflowEdge[] }) =>
      call<DagValidation>("validate_dag", { input }),
    run: (input: { workflow_id: string; params: Record<string, string> }) =>
      call<RunWorkflowResponse>("run_workflow", { input }),
    cancelExecution: (executionId: string) =>
      call<boolean>("cancel_workflow_execution", { executionId }),
    listNodeTypes: () => call<NodeTypeMeta[]>("list_node_types"),
    getNodeSchema: (typeId: string) => call<NodeTypeMeta>("get_node_schema", { typeId }),
  },
  execution: {
    preview: (input: RunCommandInput) => call<CommandPreview>("preview_command", { input }),
    run: (input: RunCommandInput) => call<RunCommandResponse>("run_command", { input }),
    cancel: (executionId: string) =>
      call<boolean>("cancel_execution", { executionId }),
    list: (params?: { limit?: number; status?: string }) =>
      call<ExecutionSummary[]>("list_executions", params),
    get: (id: string) => call<ExecutionDetail>("get_execution", { id }),
  },
  schedule: {
    list: () => call<ScheduleWithWorkflow[]>("list_schedules"),
    create: (input: CreateScheduleInput) => call<string>("create_schedule", { input }),
    update: (input: UpdateScheduleInput) => call<void>("update_schedule", { input }),
    remove: (id: string) => call<void>("delete_schedule", { id }),
    triggerNow: (id: string) => call<string>("trigger_schedule_now", { id }),
    validateCron: (expr: string) => call<void>("validate_cron_expression", { expr }),
    describeCron: (expr: string) => call<string>("describe_cron_expression", { expr }),
  },
  history: {
    list: (params?: { limit?: number; workflow_id?: string; status?: string }) =>
      call<HistorySummary[]>("list_history", params),
    get: (id: string) => call<HistoryDetail>("get_history_detail", { id }),
    replay: (id: string) => call<string>("replay_execution", { executionId: id }),
    search: (query: string, limit?: number) =>
      call<HistorySummary[]>("search_history", { query, limit }),
    stats: () => call<HistoryStats>("get_history_stats"),
    remove: (id: string) => call<void>("delete_history", { id }),
    clear: (input: ClearHistoryInput) =>
      call<ClearHistoryResult>("clear_history", { input }),
  },
  category: {
    tree: () => call<CategoryNode[]>("list_category_tree"),
    create: (input: CreateCategoryInput) => call<string>("create_category", { input }),
    rename: (id: string, name: string) =>
      call<void>("rename_category", { id, input: { id, name } }),
    move: (id: string, new_parent_id: string | null) =>
      call<void>("move_category", { id, input: { id, new_parent_id } }),
    remove: (id: string) => call<void>("delete_category", { id }),
    moveCommand: (command_id: string, category_id: string | null) =>
      call<void>("move_command", { input: { command_id, category_id } }),
    reorder: (ordered_ids: string[]) =>
      call<void>("reorder_categories", { input: { ordered_ids } }),
  },
  favorite: {
    list: () => call<Favorite[]>("list_favorites"),
    add: (command_id: string) => call<void>("add_favorite", { commandId: command_id }),
    remove: (command_id: string) => call<void>("remove_favorite", { commandId: command_id }),
    reorder: (command_ids: string[]) =>
      call<void>("reorder_favorites", { input: { command_ids } }),
  },
  plugin: {
    list: () => call<PluginInfo[]>("list_plugins"),
    remove: (id: string) => call<void>("uninstall_plugin", { id }),
    toggle: (id: string, enabled: boolean) =>
      call<void>("toggle_plugin", { id, enabled }),
    execute: (plugin_id: string, func: string, args: unknown) =>
      call<unknown>("execute_js_plugin", { pluginId: plugin_id, function: func, args }),
    executeWasm: (plugin_id: string, func: string, args: unknown) =>
      call<unknown>("execute_wasm_plugin", { pluginId: plugin_id, function: func, args }),
  },
  yaml: {
    exportWorkflow: (id: string) => call<string>("export_workflow", { id }),
    importWorkflow: (yaml: string) => call<YamlImportResult>("import_workflow", { yaml }),
  },
  template: {
    list: () => call<TemplateSummary[]>("list_templates"),
    get: (id: string) => call<string>("get_template", { id }),
    import: (id: string, new_name?: string) =>
      call<YamlImportResult>("import_template", { id, newName: new_name }),
  },
};

// ==================== 模板 (Phase 9.2) ====================

export interface TemplateSummary {
  id: string;
  name: string;
  description: string;
  category: string;
  tags: string[];
  icon: string;
  node_count: number;
  edge_count: number;
  builtin: boolean;
}

export interface CreateCommandInput {
  name: string;
  description?: string;
  category?: string;
  type: CommandType;
  template: string;
  working_dir?: string;
  env?: Record<string, string>;
  timeout_ms?: number;
  shell?: string;
  tags?: string[];
  params?: CreateParamInput[];
}

export interface CreateParamInput {
  name: string;
  label: string;
  type: ParamType;
  required: boolean;
  default_value?: unknown;
  options?: unknown;
  validation?: unknown;
  sensitive: boolean;
  description?: string;
  sort_order: number;
}

export interface UpdateCommandInput {
  id: string;
  name?: string;
  description?: string;
  category?: string;
  tags?: string[];
  template?: string;
  working_dir?: string;
  env?: Record<string, string>;
  timeout_ms?: number;
  shell?: string;
  params?: CreateParamInput[];
}

export interface CreateWorkflowInput {
  name: string;
  description?: string;
  trigger_type?: string;
}

export interface UpdateWorkflowInput {
  id: string;
  name?: string;
  description?: string;
  enabled?: boolean;
  trigger_type?: string;
  nodes?: WorkflowNode[];
  edges?: WorkflowEdge[];
}

export interface RunCommandInput {
  command_id: string;
  params: Record<string, unknown>;
  override_safety?: boolean;
}

export interface RunWorkflowResponse {
  execution_id: string;
}

export interface NodeTypeMeta {
  type_id: string;
  display_name: string;
  category: string;
  description: string;
  config_schema: unknown;
}
