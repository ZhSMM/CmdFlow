import { useState, useMemo, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  History as HistoryIcon, RefreshCw, CheckCircle2, XCircle, Loader2,
  Calendar, Clock, AlertCircle, Play, Search, X,
  Trash2, BarChart3, SearchX, Database, Terminal,
} from "lucide-react";
import {
  api, type HistorySummary, type HistoryDetail, type HistoryStats, type ClearHistoryInput,
  TauriError,
} from "@/lib/tauri";
import { Card, CardContent } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Badge } from "@/components/ui/Badge";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { formatDate, formatDuration, cn } from "@/lib/utils";

/** 把一条历史转换成 Runner 页面 URL（带 cmd + params 预填） */
function buildRunnerUrl(h: Pick<HistorySummary, "workflow_id"> & { input_params?: unknown }) {
  const params = h.input_params ?? null;
  return params
    ? `/runner?cmd=${encodeURIComponent(h.workflow_id)}&params=${encodeURIComponent(
        JSON.stringify(params),
      )}`
    : `/runner?cmd=${encodeURIComponent(h.workflow_id)}`;
}

type Filter = { status?: string; workflow_id?: string };

export function HistoryPage() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [filter, setFilter] = useState<Filter>({});
  const [searchQuery, setSearchQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [viewingId, setViewingId] = useState<string | null>(null);
  const [activeNodeRun, setActiveNodeRun] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [showClear, setShowClear] = useState(false);
  const [showStats, setShowStats] = useState(true);

  // debounce 搜索词 (300ms)
  useEffect(() => {
    const t = setTimeout(() => setDebouncedQuery(searchQuery.trim()), 300);
    return () => clearTimeout(t);
  }, [searchQuery]);

  // 搜索 vs 列表
  const isSearching = debouncedQuery.length > 0;
  const list = useQuery({
    queryKey: ["history", filter],
    queryFn: () => api.history.list({ ...filter, limit: 100 }),
    refetchInterval: autoRefresh && !isSearching ? 5000 : false,
    enabled: !isSearching,
  });
  const search = useQuery({
    queryKey: ["history-search", debouncedQuery],
    queryFn: () => api.history.search(debouncedQuery, 100),
    enabled: isSearching,
  });

  const data = isSearching ? search.data : list.data;
  const isLoading = isSearching ? search.isLoading : list.isLoading;
  const queryError = isSearching ? search.error : list.error;

  // 统计
  const stats = useQuery({
    queryKey: ["history-stats"],
    queryFn: () => api.history.stats(),
    refetchInterval: autoRefresh ? 5000 : false,
  });

  const detail = useQuery({
    queryKey: ["history-detail", viewingId],
    queryFn: () => api.history.get(viewingId!),
    enabled: !!viewingId,
  });

  const replay = useMutation({
    mutationFn: (id: string) => api.history.replay(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["history"] });
      qc.invalidateQueries({ queryKey: ["history-stats"] });
    },
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.history.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["history"] });
      qc.invalidateQueries({ queryKey: ["history-stats"] });
      setDeletingId(null);
    },
  });

  const clear = useMutation({
    mutationFn: (input: ClearHistoryInput) => api.history.clear(input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["history"] });
      qc.invalidateQueries({ queryKey: ["history-stats"] });
      setShowClear(false);
    },
  });

  const goRunner = (h: HistorySummary) => {
    navigate(buildRunnerUrl(h as HistorySummary & { input_params?: unknown }));
  };

  const usedWorkflowIds = useMemo(() => {
    const set = new Set<string>();
    (data ?? []).forEach((h) => set.add(h.workflow_id));
    return Array.from(set);
  }, [data]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">历史</h1>
          <p className="text-xs text-muted-foreground">
            所有执行的记录与日志，点「再次执行」直接跳到 Runner 预填参数
          </p>
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            size="sm"
            variant={autoRefresh ? "default" : "outline"}
            onClick={() => setAutoRefresh(!autoRefresh)}
            title="自动刷新"
          >
            <RefreshCw className={cn("h-3 w-3", autoRefresh && "animate-spin-slow")} />
            {autoRefresh ? "自动刷新" : "已暂停"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => setShowStats(!showStats)}
            title="显示统计"
          >
            <BarChart3 className="h-3 w-3" />
            统计
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => setShowClear(true)}
            disabled={!stats.data || stats.data.total === 0}
            title="清理历史"
          >
            <Trash2 className="h-3 w-3" />
            清理
          </Button>
        </div>
      </div>

      {/* 统计卡片 */}
      {showStats && stats.data && stats.data.total > 0 && (
        <StatsCards stats={stats.data} />
      )}

      {/* 搜索 + 状态筛选 */}
      <div className="flex flex-wrap items-center gap-2 border-b bg-card px-6 py-2">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="搜索 ID / 名称 / 错误信息..."
            className="h-8 w-72 pl-7 pr-7 text-xs"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery("")}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
        <div className="ml-2 flex items-center gap-1">
          <FilterChip label="全部" active={!filter.status && !filter.workflow_id} onClick={() => setFilter({})} />
          <FilterChip
            label="成功"
            active={filter.status === "success"}
            onClick={() => setFilter({ ...filter, status: "success" })}
          />
          <FilterChip
            label="失败"
            active={filter.status === "failed"}
            onClick={() => setFilter({ ...filter, status: "failed" })}
          />
          <FilterChip
            label="运行中"
            active={filter.status === "running"}
            onClick={() => setFilter({ ...filter, status: "running" })}
          />
        </div>
        {usedWorkflowIds.length > 1 && (
          <select
            className="ml-auto h-8 rounded-md border border-input bg-transparent px-2 text-xs"
            value={filter.workflow_id ?? ""}
            onChange={(e) =>
              setFilter({ ...filter, workflow_id: e.target.value || undefined })
            }
          >
            <option value="">所有工作流</option>
            {usedWorkflowIds.map((id) => (
              <option key={id} value={id}>
                {id.slice(0, 8)}…
              </option>
            ))}
          </select>
        )}
      </div>

      <div className="flex-1 overflow-auto p-6">
        {isLoading && <div className="text-sm text-muted-foreground">加载中...</div>}
        {queryError && (
          <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            {(queryError as TauriError).message}
          </div>
        )}
        {data && data.length === 0 && (
          <EmptyState
            icon={isSearching ? SearchX : HistoryIcon}
            title={isSearching ? "没找到匹配的执行" : "暂无执行记录"}
            description={
              isSearching
                ? `搜索 "${debouncedQuery}" 没有结果`
                : "执行命令或工作流后会显示在这里"
            }
            action={
              !isSearching && (
                <Button onClick={() => navigate("/library")}>
                  去看命令库
                </Button>
              )
            }
          />
        )}
        {data && data.length > 0 && (
          <div className="space-y-2">
            {data.map((h: HistorySummary) => (
              <HistoryCard
                key={h.id}
                h={h}
                onView={() => setViewingId(h.id)}
                onReplay={() => replay.mutate(h.id)}
                onRerun={() => goRunner(h)}
                onDelete={() => setDeletingId(h.id)}
                isReplaying={replay.isPending}
              />
            ))}
          </div>
        )}
      </div>

      {/* 详情弹窗 */}
      <Dialog
        open={!!viewingId}
        onOpenChange={(o) => {
          if (!o) {
            setViewingId(null);
            setActiveNodeRun(null);
          }
        }}
        title={detail.data ? `执行详情: ${detail.data.workflow_name}` : "执行详情"}
        className="max-w-4xl"
      >
        {detail.isLoading && <div className="text-sm text-muted-foreground">加载中...</div>}
        {detail.error && (
          <div className="text-sm text-destructive">{(detail.error as TauriError).message}</div>
        )}
        {detail.data && (
          <HistoryDetailView
            detail={detail.data}
            activeNodeRun={activeNodeRun}
            onSelectNodeRun={setActiveNodeRun}
          />
        )}
      </Dialog>

      {/* 删除单条 */}
      <Dialog
        open={!!deletingId}
        onOpenChange={(o) => !o && setDeletingId(null)}
        title="删除这条历史?"
        description="删除后无法恢复,相关的节点日志会一并清理。"
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeletingId(null)}>取消</Button>
            <Button
              variant="destructive"
              disabled={remove.isPending}
              onClick={() => deletingId && remove.mutate(deletingId)}
            >
              {remove.isPending ? "删除中..." : "确认删除"}
            </Button>
          </>
        }
      >
        <p className="text-sm">这只会删除历史记录,不会影响已注册的命令或工作流。</p>
      </Dialog>

      {/* 清理历史 */}
      <ClearHistoryDialog
        open={showClear}
        onOpenChange={setShowClear}
        total={stats.data?.total ?? 0}
        onConfirm={(input) => clear.mutate(input)}
        isPending={clear.isPending}
      />
    </div>
  );
}

function HistoryCard({
  h, onView, onReplay, onRerun, onDelete, isReplaying,
}: {
  h: HistorySummary;
  onView: () => void;
  onReplay: () => void;
  onRerun: () => void;
  onDelete: () => void;
  isReplaying: boolean;
}) {
  // 紧凑展示参数
  const paramsSummary = formatParamsSummary(h.input_params);
  const hasCommand = !!h.rendered_template;
  return (
    <Card className="group cursor-pointer hover:bg-accent/30" onClick={onView}>
      <CardContent className="flex items-center justify-between gap-3 p-3">
        <div className="flex min-w-0 items-center gap-3">
          <StatusIcon status={h.status} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <div className="truncate text-sm font-medium">{h.workflow_name}</div>
              {hasCommand && (
                <Badge variant="secondary" className="shrink-0 text-[10px]">
                  命令
                </Badge>
              )}
            </div>
            {/* 渲染后的命令模板 (单命令运行时) */}
            {hasCommand && (
              <div className="mt-0.5 flex items-center gap-1 truncate text-[11px] text-muted-foreground">
                <Terminal className="h-3 w-3 shrink-0" />
                <code className="truncate font-mono">{h.rendered_template}</code>
              </div>
            )}
            {/* 输入参数摘要 */}
            {paramsSummary && (
              <div className="mt-0.5 flex items-center gap-1 truncate text-[11px] text-muted-foreground">
                <span className="shrink-0">参数:</span>
                <code className="truncate font-mono">{paramsSummary}</code>
              </div>
            )}
            <div className="mt-1 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
              <span className="flex items-center gap-1">
                <Calendar className="h-3 w-3" />
                {formatDate(h.started_at)}
              </span>
              <span className="flex items-center gap-1">
                <Clock className="h-3 w-3" />
                {formatDuration(h.duration_ms)}
              </span>
              <Badge variant="outline" className="text-[10px]">
                {h.trigger === "manual" ? "手动" : h.trigger === "schedule" ? "调度" : h.trigger}
              </Badge>
            </div>
            {h.error && (
              <div className="mt-1 flex items-center gap-1 truncate text-xs text-destructive">
                <AlertCircle className="h-3 w-3 shrink-0" />
                <span className="truncate">{h.error}</span>
              </div>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button
            size="sm"
            variant="outline"
            onClick={(e) => {
              e.stopPropagation();
              onReplay();
            }}
            disabled={isReplaying}
            title="原地重放（用同参数立即重新跑一次）"
          >
            {isReplaying ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RefreshCw className="h-3 w-3" />
            )}
            重放
          </Button>
          <Button
            size="sm"
            onClick={(e) => {
              e.stopPropagation();
              onRerun();
            }}
            title="跳到执行页，预填该次参数（可改后再跑）"
          >
            <Play className="h-3 w-3" />
            再次执行
          </Button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
            className="rounded p-1.5 text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100"
            title="删除"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </CardContent>
    </Card>
  );
}

/** 把 input_params 压缩成一行短串, 太长就截断 */
function formatParamsSummary(params: unknown): string | null {
  if (params == null) return null;
  if (typeof params === "string") return params.length > 80 ? params.slice(0, 80) + "…" : params;
  if (typeof params !== "object") return String(params);
  const entries = Object.entries(params as Record<string, unknown>);
  if (entries.length === 0) return null;
  const parts = entries.map(([k, v]) => {
    const vs = typeof v === "string" ? v : JSON.stringify(v);
    return `${k}=${vs.length > 30 ? vs.slice(0, 30) + "…" : vs}`;
  });
  const joined = parts.join(", ");
  return joined.length > 100 ? joined.slice(0, 100) + "…" : joined;
}

function StatsCards({ stats }: { stats: HistoryStats }) {
  const successRate = stats.total > 0
    ? ((stats.success / stats.total) * 100).toFixed(1)
    : "0";

  return (
    <div className="grid grid-cols-2 gap-3 border-b bg-card px-6 py-3 md:grid-cols-6">
      <StatCard label="总执行" value={stats.total.toString()} />
      <StatCard
        label="成功率"
        value={`${successRate}%`}
        valueClass={Number(successRate) >= 90 ? "text-green-600" : Number(successRate) >= 70 ? "text-amber-600" : "text-destructive"}
      />
      <StatCard label="成功" value={stats.success.toString()} valueClass="text-green-600" />
      <StatCard label="失败" value={stats.failed.toString()} valueClass={stats.failed > 0 ? "text-destructive" : "text-muted-foreground"} />
      <StatCard label="运行中" value={stats.running.toString()} valueClass={stats.running > 0 ? "text-blue-500" : "text-muted-foreground"} />
      <StatCard
        label="平均耗时"
        value={formatDuration(stats.avg_duration_ms || undefined)}
      />
    </div>
  );
}

function StatCard({
  label, value, valueClass,
}: {
  label: string;
  value: string;
  valueClass?: string;
}) {
  return (
    <div className="rounded-lg border bg-background p-2.5">
      <div className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</div>
      <div className={cn("mt-0.5 text-xl font-semibold tabular-nums", valueClass)}>
        {value}
      </div>
    </div>
  );
}

function FilterChip({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "rounded-md px-2 py-0.5 text-xs",
        active
          ? "bg-primary text-primary-foreground"
          : "bg-secondary text-secondary-foreground hover:bg-accent",
      )}
    >
      {label}
    </button>
  );
}

function StatusIcon({ status }: { status: string }) {
  if (status === "success") return <CheckCircle2 className="h-4 w-4 shrink-0 text-green-600" />;
  if (status === "running" || status === "pending") return <Loader2 className="h-4 w-4 shrink-0 animate-spin text-blue-500" />;
  if (status === "failed" || status === "cancelled" || status === "timeout")
    return <XCircle className="h-4 w-4 shrink-0 text-destructive" />;
  return <AlertCircle className="h-4 w-4 shrink-0 text-muted-foreground" />;
}

function HistoryDetailView({
  detail,
  activeNodeRun,
  onSelectNodeRun,
}: {
  detail: HistoryDetail;
  activeNodeRun: string | null;
  onSelectNodeRun: (id: string | null) => void;
}) {
  const active = detail.node_runs.find((n) => n.id === activeNodeRun) || detail.node_runs[0];
  const hasParams = detail.input_params != null && typeof detail.input_params === "object" &&
    Object.keys(detail.input_params as object).length > 0;

  return (
    <div className="space-y-3 text-xs">
      {/* 顶部:命令 + 参数 */}
      {detail.rendered_template && (
        <div>
          <div className="mb-1 text-xs font-medium">命令</div>
          <pre className="overflow-auto rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100 max-h-24">
            {detail.rendered_template}
          </pre>
        </div>
      )}
      {hasParams && (
        <div>
          <div className="mb-1 text-xs font-medium">参数</div>
          <pre className="overflow-auto rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100 max-h-24">
            {JSON.stringify(detail.input_params, null, 2)}
          </pre>
        </div>
      )}

      {/* 节点列表 + 详情 */}
      <div className="grid grid-cols-3 gap-3">
      {/* 左侧:节点列表 */}
      <div className="col-span-1 space-y-1">
        <div className="mb-1 font-medium">节点 ({detail.node_runs.length})</div>
        {detail.node_runs.length === 0 ? (
          <div className="text-muted-foreground">无节点记录</div>
        ) : (
          detail.node_runs.map((n) => (
            <div
              key={n.id}
              className={cn(
                "cursor-pointer rounded-md border p-2 hover:bg-accent/30",
                active?.id === n.id && "border-primary bg-accent/30",
              )}
              onClick={() => onSelectNodeRun(n.id)}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-1.5 truncate">
                  <StatusIcon status={n.status} />
                  <code className="truncate text-[10px]">{n.node_id.slice(0, 8)}</code>
                </div>
                <span className="text-[10px] text-muted-foreground">
                  {formatDuration(n.duration_ms)}
                </span>
              </div>
              {n.error && (
                <div className="mt-1 truncate text-[10px] text-destructive">{n.error}</div>
              )}
            </div>
          ))
        )}
      </div>

      {/* 右侧:节点详情 */}
      <div className="col-span-2">
        {active ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <StatusIcon status={active.status} />
              <code className="text-[10px]">{active.node_id}</code>
              <span className="text-muted-foreground">·</span>
              <span>{active.status}</span>
              {active.exit_code !== null && (
                <>
                  <span className="text-muted-foreground">·</span>
                  <span>退出码: {active.exit_code}</span>
                </>
              )}
              <span className="text-muted-foreground">·</span>
              <span>{formatDuration(active.duration_ms)}</span>
            </div>
            {active.output_data !== null && (
              <div>
                <div className="text-xs font-medium">输出数据</div>
                <pre className="mt-1 max-h-40 overflow-auto rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
                  {JSON.stringify(active.output_data, null, 2)}
                </pre>
              </div>
            )}
            {active.stdout && (
              <div>
                <div className="text-xs font-medium">stdout</div>
                <pre className="mt-1 max-h-40 overflow-auto rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
                  {active.stdout}
                </pre>
              </div>
            )}
            {active.stderr && (
              <div>
                <div className="text-xs font-medium text-destructive">stderr</div>
                <pre className="mt-1 max-h-40 overflow-auto rounded bg-slate-900 p-2 font-mono text-[10px] text-red-300">
                  {active.stderr}
                </pre>
              </div>
            )}
            {active.error && (
              <div>
                <div className="text-xs font-medium text-destructive">错误</div>
                <pre className="mt-1 rounded bg-destructive/10 p-2 text-destructive">
                  {active.error}
                </pre>
              </div>
            )}
          </div>
        ) : (
          <div className="text-muted-foreground">选择一个节点查看详情</div>
        )}
      </div>
      </div>
    </div>
  );
}

function ClearHistoryDialog({
  open, onOpenChange, total, onConfirm, isPending,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  total: number;
  onConfirm: (input: ClearHistoryInput) => void;
  isPending: boolean;
}) {
  const [mode, setMode] = useState<"all" | "failed" | "older">("all");
  const [days, setDays] = useState(30);
  const [preview, setPreview] = useState<{ deleted: number } | null>(null);

  useEffect(() => {
    if (!open) {
      setPreview(null);
      return;
    }
    // 实时预览
    const input: ClearHistoryInput = mode === "all"
      ? {}
      : mode === "failed"
      ? { status: "failed" }
      : { before_ts: Math.floor(Date.now() / 1000) - days * 86400 };
    input.dry_run = true;
    api.history.clear(input).then(setPreview).catch(() => setPreview(null));
  }, [open, mode, days]);

  const handleConfirm = () => {
    const input: ClearHistoryInput = mode === "all"
      ? {}
      : mode === "failed"
      ? { status: "failed" }
      : { before_ts: Math.floor(Date.now() / 1000) - days * 86400 };
    onConfirm(input);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="清理历史"
      description={`当前共 ${total} 条记录`}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button
            variant="destructive"
            onClick={handleConfirm}
            disabled={isPending || !preview || preview.deleted === 0}
          >
            {isPending ? "清理中..." : `清理 ${preview?.deleted ?? 0} 条`}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-sm">
        <div className="space-y-2">
          <label className="flex items-center gap-2 rounded-md border p-2.5 hover:bg-accent/30 has-[:checked]:border-primary">
            <input
              type="radio"
              checked={mode === "all"}
              onChange={() => setMode("all")}
            />
            <div>
              <div className="font-medium">清空所有</div>
              <div className="text-xs text-muted-foreground">删除全部历史记录 (谨慎)</div>
            </div>
          </label>
          <label className="flex items-center gap-2 rounded-md border p-2.5 hover:bg-accent/30 has-[:checked]:border-primary">
            <input
              type="radio"
              checked={mode === "failed"}
              onChange={() => setMode("failed")}
            />
            <div>
              <div className="font-medium">仅失败的</div>
              <div className="text-xs text-muted-foreground">只删除 status=failed 的</div>
            </div>
          </label>
          <label className="flex items-center gap-2 rounded-md border p-2.5 hover:bg-accent/30 has-[:checked]:border-primary">
            <input
              type="radio"
              checked={mode === "older"}
              onChange={() => setMode("older")}
            />
            <div className="flex-1">
              <div className="font-medium">早于指定天数的</div>
              <div className="text-xs text-muted-foreground">保留最近的数据</div>
            </div>
            {mode === "older" && (
              <input
                type="number"
                min={1}
                value={days}
                onChange={(e) => setDays(Math.max(1, Number(e.target.value) || 30))}
                className="w-16 rounded border bg-background px-2 py-0.5 text-right text-xs"
                onClick={(e) => e.stopPropagation()}
              />
            )}
          </label>
        </div>
        <div className="flex items-center gap-2 rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs text-amber-600">
          <Database className="h-3.5 w-3.5" />
          <span>将删除 <b>{preview?.deleted ?? 0}</b> 条记录和相关节点日志</span>
        </div>
      </div>
    </Dialog>
  );
}
