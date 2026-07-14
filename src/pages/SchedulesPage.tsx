import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  CalendarClock, Plus, Trash2, Play, Power, PowerOff, Loader2,
} from "lucide-react";
import { api, type ScheduleWithWorkflow, type CreateScheduleInput, TauriError } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Badge } from "@/components/ui/Badge";
import { formatDate } from "@/lib/utils";

export function SchedulesPage() {
  const qc = useQueryClient();
  const [showNew, setShowNew] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["schedules"],
    queryFn: () => api.schedule.list(),
    refetchInterval: 30000,
  });

  const toggle = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.schedule.update({ id, enabled }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["schedules"] }),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.schedule.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["schedules"] });
      setDeletingId(null);
    },
  });

  const trigger = useMutation({
    mutationFn: (id: string) => api.schedule.triggerNow(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["history"] }),
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">调度</h1>
          <p className="text-xs text-muted-foreground">
            Cron 定时触发工作流，应用内调度 + OS 计划任务
          </p>
        </div>
        <Button onClick={() => setShowNew(true)}>
          <Plus className="h-4 w-4" />
          新建调度
        </Button>
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
            icon={CalendarClock}
            title="还没有调度"
            description="点「新建调度」配置定时任务"
          />
        )}
        {data && data.length > 0 && (
          <div className="space-y-2">
            {data.map((s: ScheduleWithWorkflow) => (
              <Card key={s.id}>
                <CardHeader className="flex flex-row items-center justify-between space-y-0">
                  <div className="space-y-1">
                    <CardTitle className="flex items-center gap-2 text-sm">
                      <CalendarClock className="h-4 w-4" />
                      {s.workflow_name}
                      {s.workflow_enabled ? (
                        <Badge variant="success" className="text-[10px]">工作流已启用</Badge>
                      ) : (
                        <Badge variant="destructive" className="text-[10px]">工作流已禁用</Badge>
                      )}
                    </CardTitle>
                    <CardDescription>
                      <code className="font-mono text-[10px]">{s.cron_expr}</code>
                      <span className="ml-2">· {s.timezone}</span>
                    </CardDescription>
                  </div>
                  <div className="flex items-center gap-1">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => trigger.mutate(s.id)}
                      disabled={trigger.isPending || !s.workflow_enabled}
                      title="立即触发"
                    >
                      {trigger.isPending ? <Loader2 className="h-3 w-3 animate-spin" /> : <Play className="h-3 w-3" />}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => toggle.mutate({ id: s.id, enabled: !s.enabled })}
                      title={s.enabled ? "禁用" : "启用"}
                    >
                      {s.enabled ? <PowerOff className="h-3 w-3" /> : <Power className="h-3 w-3" />}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setDeletingId(s.id)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="text-xs text-muted-foreground">
                  <div className="flex flex-wrap gap-4">
                    <div>
                      <span className="text-foreground/70">下次运行: </span>
                      <span>{s.next_run_at ? formatDate(s.next_run_at) : "-"}</span>
                    </div>
                    <div>
                      <span className="text-foreground/70">上次运行: </span>
                      <span>{s.last_run_at ? formatDate(s.last_run_at) : "未运行"}</span>
                    </div>
                    <div>
                      <span className="text-foreground/70">耗时: </span>
                      <span>{s.last_run_at ? "-" : "-"}</span>
                    </div>
                    <div>
                      <Badge variant="outline" className="text-[10px]">
                        模式: {s.mode}
                      </Badge>
                    </div>
                    <div>
                      {s.enabled ? (
                        <Badge variant="success" className="text-[10px]">已启用</Badge>
                      ) : (
                        <Badge variant="outline" className="text-[10px]">已禁用</Badge>
                      )}
                    </div>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>

      <CreateScheduleDialog open={showNew} onOpenChange={setShowNew} />

      <Dialog
        open={!!deletingId}
        onOpenChange={(o) => !o && setDeletingId(null)}
        title="确认删除调度"
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeletingId(null)}>取消</Button>
            <Button
              variant="destructive"
              onClick={() => deletingId && remove.mutate(deletingId)}
              disabled={remove.isPending}
            >
              {remove.isPending ? "删除中..." : "确认删除"}
            </Button>
          </>
        }
      >
        <p className="text-sm">删除后,定时触发将停止。</p>
      </Dialog>
    </div>
  );
}

function CreateScheduleDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const qc = useQueryClient();
  const [workflowId, setWorkflowId] = useState("");
  const [cronExpr, setCronExpr] = useState("0 2 * * *");
  const [mode, setMode] = useState<"in_app" | "os_native" | "hybrid">("in_app");
  const [error, setError] = useState<string | null>(null);
  const [describe, setDescribe] = useState("");

  const workflows = useQuery({
    queryKey: ["workflows"],
    queryFn: () => api.workflow.list(),
    enabled: open,
  });

  const create = useMutation({
    mutationFn: () =>
      api.schedule.create({
        workflow_id: workflowId,
        cron_expr: cronExpr,
        mode,
        enabled: true,
      } as CreateScheduleInput),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["schedules"] });
      onOpenChange(false);
      setError(null);
    },
    onError: (e) => setError((e as TauriError).message),
  });

  const handleExprChange = async (expr: string) => {
    setCronExpr(expr);
    try {
      const d = await api.schedule.describeCron(expr);
      setDescribe(d);
    } catch {
      setDescribe("");
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="新建调度"
      description="为工作流配置定时触发"
      className="max-w-lg"
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button
            onClick={() => create.mutate()}
            disabled={!workflowId || !cronExpr || create.isPending}
          >
            {create.isPending ? "创建中..." : "创建"}
          </Button>
        </>
      }
    >
      {error && (
        <div className="mb-3 rounded-md border border-destructive bg-destructive/10 p-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="space-y-3">
        <div>
          <label className="mb-1 block text-sm font-medium">工作流 *</label>
          <select
            className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm"
            value={workflowId}
            onChange={(e) => setWorkflowId(e.target.value)}
          >
            <option value="">-- 请选择 --</option>
            {(workflows.data ?? []).map((w) => (
              <option key={w.id} value={w.id}>
                {w.name} {w.enabled ? "" : "(已禁用)"}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="mb-1 block text-sm font-medium">Cron 表达式 *</label>
          <Input
            value={cronExpr}
            onChange={(e) => handleExprChange(e.target.value)}
            placeholder="0 2 * * *"
            className="font-mono"
          />
          <div className="mt-1 flex flex-wrap gap-1 text-[10px]">
            <button
              type="button"
              className="rounded bg-secondary px-1.5 py-0.5"
              onClick={() => handleExprChange("*/5 * * * *")}
            >
              每 5 分
            </button>
            <button
              type="button"
              className="rounded bg-secondary px-1.5 py-0.5"
              onClick={() => handleExprChange("0 * * * *")}
            >
              每小时
            </button>
            <button
              type="button"
              className="rounded bg-secondary px-1.5 py-0.5"
              onClick={() => handleExprChange("0 0 * * *")}
            >
              每天
            </button>
            <button
              type="button"
              className="rounded bg-secondary px-1.5 py-0.5"
              onClick={() => handleExprChange("0 9 * * 1-5")}
            >
              工作日 9 点
            </button>
            <button
              type="button"
              className="rounded bg-secondary px-1.5 py-0.5"
              onClick={() => handleExprChange("0 0 * * 0")}
            >
              每周日
            </button>
          </div>
          {describe && (
            <p className="mt-1 text-xs text-muted-foreground">→ {describe}</p>
          )}
        </div>

        <div>
          <label className="mb-1 block text-sm font-medium">调度模式</label>
          <select
            className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm"
            value={mode}
            onChange={(e) => setMode(e.target.value as any)}
          >
            <option value="in_app">应用内调度 (应用关闭则不触发)</option>
            <option value="os_native">OS 计划任务 (Windows/Mac/Linux)</option>
            <option value="hybrid">混合 (应用内优先,OS 兜底)</option>
          </select>
        </div>
      </div>
    </Dialog>
  );
}
