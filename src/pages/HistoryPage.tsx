import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  History as HistoryIcon, RefreshCw, CheckCircle2, XCircle, Loader2,
  Calendar, Clock, AlertCircle, Copy, Terminal as TerminalIcon,
} from "lucide-react";
import { api, type HistorySummary, type HistoryDetail, TauriError } from "@/lib/tauri";
import { Card, CardContent } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/Badge";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { formatDate, formatDuration, cn } from "@/lib/utils";

export function HistoryPage() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [filter, setFilter] = useState<{ status?: string; workflow_id?: string }>({});
  const [viewingId, setViewingId] = useState<string | null>(null);
  const [activeNodeRun, setActiveNodeRun] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["history", filter],
    queryFn: () => api.history.list({ ...filter, limit: 100 }),
    refetchInterval: 5000,
  });

  const detail = useQuery({
    queryKey: ["history-detail", viewingId],
    queryFn: () => api.history.get(viewingId!),
    enabled: !!viewingId,
  });

  const replay = useMutation({
    mutationFn: (id: string) => api.history.replay(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["history"] }),
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">历史</h1>
          <p className="text-xs text-muted-foreground">所有执行的记录与日志，可重放或载入参数</p>
        </div>
        <div className="flex items-center gap-1">
          <FilterChip label="全部" active={!filter.status} onClick={() => setFilter({})} />
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
      </div>

      <div className="flex-1 overflow-auto p-6">
        {isLoading && <div className="text-sm text-muted-foreground">加载中...</div>}
        {error && (
          <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            {(error as TauriError).message}
          </div>
        )}
        {data && data.length === 0 && (
          <EmptyState
            icon={HistoryIcon}
            title="暂无执行记录"
            description="执行命令或工作流后会显示在这里"
          />
        )}
        {data && data.length > 0 && (
          <div className="space-y-2">
            {data.map((h: HistorySummary) => (
              <Card
                key={h.id}
                className="cursor-pointer hover:bg-accent/30"
                onClick={() => setViewingId(h.id)}
              >
                <CardContent className="flex items-center justify-between p-3">
                  <div className="flex items-center gap-3">
                    <StatusIcon status={h.status} />
                    <div>
                      <div className="text-sm font-medium">{h.workflow_name}</div>
                      <div className="flex items-center gap-3 text-xs text-muted-foreground">
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
                        <div className="mt-1 flex items-center gap-1 text-xs text-destructive">
                          <AlertCircle className="h-3 w-3" />
                          {h.error}
                        </div>
                      )}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={(e) => {
                      e.stopPropagation();
                      replay.mutate(h.id);
                    }}
                    disabled={replay.isPending}
                  >
                    {replay.isPending ? (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    ) : (
                      <RefreshCw className="h-3 w-3" />
                    )}
                    重放
                  </Button>
                </CardContent>
              </Card>
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
        title="执行详情"
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
            onLoadToRunner={() => {
              if (!detail.data) return;
              const id = detail.data.workflow_id;
              const params = detail.data.input_params ?? null;
              const url = params
                ? `/runner?cmd=${encodeURIComponent(id)}&params=${encodeURIComponent(JSON.stringify(params))}`
                : `/runner?cmd=${encodeURIComponent(id)}`;
              setViewingId(null);
              navigate(url);
            }}
          />
        )}
      </Dialog>
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
  if (status === "success") return <CheckCircle2 className="h-4 w-4 text-green-600" />;
  if (status === "running" || status === "pending") return <Loader2 className="h-4 w-4 animate-spin text-blue-500" />;
  if (status === "failed" || status === "cancelled" || status === "timeout")
    return <XCircle className="h-4 w-4 text-destructive" />;
  return <AlertCircle className="h-4 w-4 text-muted-foreground" />;
}

function HistoryDetailView({
  detail,
  activeNodeRun,
  onSelectNodeRun,
  onLoadToRunner,
}: {
  detail: HistoryDetail;
  activeNodeRun: string | null;
  onSelectNodeRun: (id: string | null) => void;
  onLoadToRunner: () => void;
}) {
  const active = detail.node_runs.find((n) => n.id === activeNodeRun) || detail.node_runs[0];
  const hasInputParams = Boolean(
    detail.input_params &&
      typeof detail.input_params === "object" &&
      Object.keys(detail.input_params).length > 0,
  );

  return (
    <div className="space-y-3 text-xs">
      {/* 顶部：摘要 + 操作 */}
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border bg-muted/30 p-2">
        <div className="flex flex-wrap items-center gap-2">
          <StatusIcon status={detail.status} />
          <span className="font-medium">{detail.workflow_name}</span>
          <span className="text-muted-foreground">·</span>
          <span className="text-muted-foreground">{formatDate(detail.started_at)}</span>
          <span className="text-muted-foreground">·</span>
          <span className="text-muted-foreground">{formatDuration(detail.duration_ms)}</span>
          <Badge variant="outline" className="text-[10px]">
            {detail.trigger === "manual" ? "手动" : detail.trigger === "schedule" ? "调度" : detail.trigger}
          </Badge>
        </div>
        {hasInputParams && (
          <Button size="sm" variant="outline" onClick={onLoadToRunner}>
            <Copy className="h-3 w-3" />
            载入到执行页
          </Button>
        )}
      </div>

      {detail.error ? (
        <div className="rounded-md border border-destructive bg-destructive/10 p-2 text-destructive">
          <div className="font-medium">顶层错误</div>
          <pre className="mt-1 whitespace-pre-wrap text-xs">{String(detail.error)}</pre>
        </div>
      ) : null}

      {/* 命令参数 */}
      {hasInputParams ? (
        <div>
          <div className="mb-1 font-medium">命令参数</div>
          <pre className="max-h-40 overflow-auto rounded bg-slate-900 p-2 font-mono text-[11px] text-slate-100">
            {JSON.stringify(detail.input_params, null, 2) as string}
          </pre>
        </div>
      ) : null}

      {/* 节点列表 + 节点详情 */}
      <div className="grid grid-cols-3 gap-3">
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
              {active.output_data !== null && active.output_data !== undefined && (
                <div>
                  <div className="text-xs font-medium">输出数据</div>
                  <pre className="mt-1 max-h-40 overflow-auto rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
                    {JSON.stringify(active.output_data as unknown, null, 2)}
                  </pre>
                </div>
              )}
              {active.stdout && (
                <div>
                  <div className="flex items-center gap-1 text-xs font-medium">
                    <TerminalIcon className="h-3 w-3" />
                    stdout
                  </div>
                  <pre className="mt-1 max-h-60 overflow-auto whitespace-pre-wrap rounded bg-slate-900 p-2 font-mono text-[11px] text-slate-100">
                    {active.stdout}
                  </pre>
                </div>
              )}
              {active.stderr && (
                <div>
                  <div className="text-xs font-medium text-destructive">stderr</div>
                  <pre className="mt-1 max-h-60 overflow-auto whitespace-pre-wrap rounded bg-slate-900 p-2 font-mono text-[11px] text-red-300">
                    {active.stderr}
                  </pre>
                </div>
              )}
              {active.error && (
                <div>
                  <div className="text-xs font-medium text-destructive">节点错误</div>
                  <pre className="mt-1 whitespace-pre-wrap rounded bg-destructive/10 p-2 text-destructive">
                    {active.error}
                  </pre>
                </div>
              )}
              {!active.stdout && !active.stderr && !active.error && !active.output_data && (
                <div className="text-muted-foreground">该节点无输出</div>
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
