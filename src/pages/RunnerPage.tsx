import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Play,
  Square,
  Eye,
  AlertTriangle,
  XCircle,
  Loader2,
  CheckCircle2,
  History as HistoryIcon,
} from "lucide-react";
import { api, onRunEvent, type RunEvent, type CommandPreview, type CommandDetail, TauriError } from "@/lib/tauri";
import { Button } from "@/components/ui/Button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { ParamField } from "@/components/forms/ParamField";
import { Dialog } from "@/components/ui/Dialog";
import { Terminal, type TerminalHandle } from "@/components/runner/Terminal";
import { cn, formatDuration } from "@/lib/utils";

type RunState = "idle" | "previewing" | "ready" | "running" | "success" | "failed" | "cancelled" | "timeout";

export function RunnerPage() {
  const [params] = useSearchParams();
  const initialCmdId = params.get("cmd");
  const qc = useQueryClient();

  const [commandId, setCommandId] = useState<string | null>(initialCmdId);
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [preview, setPreview] = useState<CommandPreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [showPreview, setShowPreview] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const [runState, setRunState] = useState<RunState>("idle");
  const [executionId, setExecutionId] = useState<string | null>(null);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const termRef = useRef<TerminalHandle>(null);
  const isMounted = useRef(true);

  const commands = useQuery({
    queryKey: ["commands"],
    queryFn: () => api.library.list(),
  });

  const detail = useQuery({
    queryKey: ["command", commandId],
    queryFn: () => api.library.get(commandId!),
    enabled: !!commandId,
  });

  // 切换命令时重置
  useEffect(() => {
    setValues({});
    setPreview(null);
    setPreviewError(null);
    setRunState("idle");
    setExecutionId(null);
    setError(null);
    setExitCode(null);
  }, [commandId]);

  useEffect(() => {
    return () => {
      isMounted.current = false;
    };
  }, []);

  // 订阅执行事件
  useEffect(() => {
    if (!executionId) return;

    const unlistenPromise = onRunEvent((e: RunEvent) => {
      if ((e as any).execution_id !== executionId) return;
      switch (e.kind) {
        case "execution_started":
          termRef.current?.clear();
          termRef.current?.writeln(
            `\x1b[36m▶ 开始执行: ${e.command_name}\x1b[0m\r\n`,
          );
          break;
        case "node_log":
          if (e.stream === "stderr") {
            termRef.current?.write(`\x1b[31m${e.content}\x1b[0m`);
          } else if (e.stream === "system") {
            termRef.current?.write(`\x1b[33m${e.content}\x1b[0m`);
          } else {
            termRef.current?.write(e.content);
          }
          break;
        case "node_finished":
          setExitCode(e.exit_code);
          break;
        case "execution_finished":
          setRunState(e.status as RunState);
          setError(e.error);
          termRef.current?.writeln(
            `\r\n\x1b[${e.status === "success" ? "32" : "31"}m■ 执行结束: ${e.status} · ${formatDuration(e.duration_ms)}\x1b[0m`,
          );
          qc.invalidateQueries({ queryKey: ["executions"] });
          break;
        case "execution_cancelled":
          termRef.current?.writeln(`\r\n\x1b[33m■ 已取消\x1b[0m`);
          break;
      }
    });

    return () => {
      unlistenPromise.then((u) => u());
    };
  }, [executionId, qc]);

  const doPreview = async () => {
    if (!commandId) return;
    setPreviewError(null);
    setRunState("previewing");
    try {
      const p = await api.execution.preview({
        command_id: commandId,
        params: values,
      });
      setPreview(p);
      setShowPreview(true);
      setRunState("ready");
    } catch (e) {
      setPreviewError((e as TauriError).message);
      setRunState("idle");
    }
  };

  const doRun = async (override = false) => {
    if (!commandId) return;
    setError(null);
    setRunState("running");
    setShowConfirm(false);
    try {
      const res = await api.execution.run({
        command_id: commandId,
        params: values,
        override_safety: override,
      });
      setExecutionId(res.execution_id);
    } catch (e) {
      setError((e as TauriError).message);
      setRunState("failed");
    }
  };

  const doCancel = async () => {
    if (!executionId) return;
    await api.execution.cancel(executionId);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">执行</h1>
          <p className="text-xs text-muted-foreground">
            选择命令、填参数、预览、运行
          </p>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* 左:命令选择 + 参数 */}
        <div className="flex w-1/2 flex-col border-r">
          <div className="border-b p-4">
            <label className="mb-1 block text-sm font-medium">选择命令</label>
            <select
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm"
              value={commandId ?? ""}
              onChange={(e) => setCommandId(e.target.value || null)}
            >
              <option value="">-- 请选择 --</option>
              {(commands.data ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name} ({c.type})
                </option>
              ))}
            </select>
          </div>

          <div className="flex-1 overflow-auto p-4">
            {!commandId && (
              <EmptyState
                icon={HistoryIcon}
                title="先选一个命令"
                description="或在「命令库」点击 ▶ 按钮"
              />
            )}
            {commandId && detail.isLoading && (
              <div className="text-sm text-muted-foreground">加载命令详情...</div>
            )}
            {commandId && detail.error && (
              <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                {(detail.error as TauriError).message}
              </div>
            )}
            {commandId && detail.data && (
              <CommandParamForm
                detail={detail.data}
                values={values}
                onChange={(name, v) => setValues((prev) => ({ ...prev, [name]: v }))}
                disabled={runState === "running"}
              />
            )}
          </div>

          <div className="flex items-center gap-2 border-t p-3">
            <Button
              variant="outline"
              onClick={doPreview}
              disabled={!commandId || runState === "running" || runState === "previewing"}
            >
              <Eye className="h-4 w-4" />
              预览
            </Button>
            <Button
              onClick={() => doRun(false)}
              disabled={!commandId || runState === "running"}
            >
              {runState === "running" ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  执行中...
                </>
              ) : (
                <>
                  <Play className="h-4 w-4" />
                  执行
                </>
              )}
            </Button>
            {runState === "running" && (
              <Button variant="destructive" onClick={doCancel}>
                <Square className="h-4 w-4" />
                取消
              </Button>
            )}
            {error && (
              <div className="flex-1 truncate text-xs text-destructive">{error}</div>
            )}
            {!error && exitCode !== null && runState !== "running" && (
              <div
                className={cn(
                  "flex-1 truncate text-xs",
                  runState === "success" ? "text-green-600" : "text-destructive",
                )}
              >
                退出码: {exitCode} · {runState} · {formatDuration(detail.data?.version.timeout_ms ?? undefined)}
              </div>
            )}
          </div>
        </div>

        {/* 右:终端 + 历史 */}
        <div className="flex w-1/2 flex-col">
          <div className="flex-1 overflow-hidden p-4">
            <Terminal ref={termRef} />
          </div>
          <ExecutionHistoryMini commandId={commandId} />
        </div>
      </div>

      {/* 预览弹窗 */}
      <Dialog
        open={showPreview}
        onOpenChange={setShowPreview}
        title="命令预览"
        description="这是将要在 shell 中执行的最终命令"
        className="max-w-2xl"
        footer={
          <>
            <Button variant="ghost" onClick={() => setShowPreview(false)}>
              关闭
            </Button>
            <Button
              onClick={() => {
                setShowPreview(false);
                if (preview && (preview.blacklisted || preview.dangerous)) {
                  setShowConfirm(true);
                } else {
                  doRun(false);
                }
              }}
            >
              <Play className="h-4 w-4" />
              执行
            </Button>
          </>
        }
      >
        {previewError && (
          <div className="mb-3 rounded-md border border-destructive bg-destructive/10 p-2 text-sm text-destructive">
            {previewError}
          </div>
        )}
        {preview && (
          <div className="space-y-3 text-sm">
            <div>
              <div className="mb-1 text-xs font-medium text-muted-foreground">
                最终命令
              </div>
              <pre className="overflow-auto rounded-md bg-slate-900 p-3 font-mono text-xs text-slate-100">
                {preview.rendered}
              </pre>
            </div>
            {preview.cwd && (
              <div>
                <span className="text-xs text-muted-foreground">工作目录: </span>
                <code className="text-xs">{preview.cwd}</code>
              </div>
            )}
            {preview.timeout_ms && (
              <div>
                <span className="text-xs text-muted-foreground">超时: </span>
                <code className="text-xs">{formatDuration(preview.timeout_ms)}</code>
              </div>
            )}
            {Object.keys(preview.env).length > 0 && (
              <div>
                <div className="mb-1 text-xs font-medium text-muted-foreground">
                  环境变量
                </div>
                <pre className="overflow-auto rounded-md bg-slate-900 p-3 font-mono text-xs text-slate-100">
                  {Object.entries(preview.env)
                    .map(([k, v]) => `${k}=${v}`)
                    .join("\n")}
                </pre>
              </div>
            )}
            {preview.warnings.length > 0 && (
              <div className="rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs">
                {preview.warnings.map((w, i) => (
                  <div key={i} className="flex items-start gap-1.5">
                    <AlertTriangle className="mt-0.5 h-3.5 w-3.5 flex-shrink-0 text-amber-500" />
                    <span>{w}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </Dialog>

      {/* 危险确认 */}
      <Dialog
        open={showConfirm}
        onOpenChange={setShowConfirm}
        title="危险命令确认"
        footer={
          <>
            <Button variant="ghost" onClick={() => setShowConfirm(false)}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setShowConfirm(false);
                doRun(true);
              }}
            >
              <AlertTriangle className="h-4 w-4" />
              我已了解风险，继续执行
            </Button>
          </>
        }
      >
        <div className="space-y-2 text-sm">
          {preview?.blacklisted && (
            <div className="flex items-start gap-2 rounded-md border border-destructive bg-destructive/10 p-2 text-destructive">
              <XCircle className="mt-0.5 h-4 w-4 flex-shrink-0" />
              <div>
                <div className="font-medium">命令命中黑名单</div>
                <div className="text-xs opacity-80">{preview.blacklisted}</div>
              </div>
            </div>
          )}
          {preview?.dangerous && !preview.blacklisted && (
            <div className="flex items-start gap-2 rounded-md border border-amber-500 bg-amber-500/10 p-2 text-amber-600">
              <AlertTriangle className="mt-0.5 h-4 w-4 flex-shrink-0" />
              <div>
                <div className="font-medium">检测到危险操作</div>
                <div className="text-xs opacity-80">
                  命令可能造成不可逆的更改（删除/格式化/强制推送等），请确认意图
                </div>
              </div>
            </div>
          )}
          <p className="text-xs text-muted-foreground">
            点击「继续执行」将以 <code className="rounded bg-secondary px-1">override_safety</code> 方式执行，记录会标记为「用户已确认」。
          </p>
        </div>
      </Dialog>
    </div>
  );
}

function CommandParamForm({
  detail,
  values,
  onChange,
  disabled,
}: {
  detail: CommandDetail;
  values: Record<string, unknown>;
  onChange: (name: string, value: unknown) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">命令信息</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1 text-xs">
          <div>
            <span className="text-muted-foreground">类型: </span>
            <code>{detail.type}</code>
          </div>
          <div>
            <span className="text-muted-foreground">模板: </span>
            <code className="break-all">{detail.version.template}</code>
          </div>
          {detail.version.timeout_ms && (
            <div>
              <span className="text-muted-foreground">超时: </span>
              <code>{formatDuration(detail.version.timeout_ms)}</code>
            </div>
          )}
        </CardContent>
      </Card>

      {detail.description && (
        <p className="text-xs text-muted-foreground">{detail.description}</p>
      )}

      {detail.params.length === 0 ? (
        <p className="text-xs text-muted-foreground">此命令没有参数。</p>
      ) : (
        <div className={cn("space-y-3", disabled && "pointer-events-none opacity-50")}>
          {detail.params.map((p) => (
            <ParamField
              key={p.id}
              param={p}
              value={values[p.name]}
              onChange={(v) => onChange(p.name, v)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ExecutionHistoryMini({ commandId }: { commandId: string | null }) {
  const { data } = useQuery({
    queryKey: ["executions", { commandId }],
    queryFn: () => api.execution.list({ limit: 5 }),
  });

  return (
    <div className="border-t p-4">
      <div className="mb-2 text-xs font-medium text-muted-foreground">最近执行</div>
      <div className="space-y-1 text-xs">
        {(data ?? []).length === 0 ? (
          <div className="text-muted-foreground">暂无记录</div>
        ) : (
          (data ?? []).map((e) => (
            <div
              key={e.id}
              className="flex items-center justify-between rounded px-2 py-1 hover:bg-accent/50"
            >
              <div className="flex items-center gap-2 truncate">
                {e.status === "success" ? (
                  <CheckCircle2 className="h-3 w-3 text-green-600" />
                ) : e.status === "running" ? (
                  <Loader2 className="h-3 w-3 animate-spin text-blue-500" />
                ) : (
                  <XCircle className="h-3 w-3 text-destructive" />
                )}
                <span className="truncate">{e.command_name}</span>
              </div>
              <span className="text-muted-foreground">
                {formatDuration(e.duration_ms ?? undefined)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
