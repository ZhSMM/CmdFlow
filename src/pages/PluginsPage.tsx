import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Puzzle, Power, PowerOff, Trash2, RefreshCw, AlertCircle, Play, Loader2,
} from "lucide-react";
import { api, type PluginInfo, TauriError } from "@/lib/tauri";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/Badge";
import { EmptyState } from "@/components/ui/EmptyState";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";

export function PluginsPage() {
  const qc = useQueryClient();
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [testing, setTesting] = useState<PluginInfo | null>(null);

  const { data, isLoading, error, isFetching, refetch } = useQuery({
    queryKey: ["plugins"],
    queryFn: () => api.plugin.list(),
  });

  const toggle = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.plugin.toggle(id, enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["plugins"] }),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.plugin.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["plugins"] });
      setDeletingId(null);
    },
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">插件</h1>
          <p className="text-xs text-muted-foreground">
            扩展 CmdFlow 能力。JS / WASM 插件放 <code>%APPDATA%\dev.cmdflow.app\plugins\</code>
          </p>
        </div>
        <Button
          variant="outline"
          onClick={() => refetch()}
          disabled={isFetching}
        >
          <RefreshCw className={isFetching ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
          扫描
        </Button>
      </div>

      <div className="flex-1 overflow-auto p-6">
        {isLoading && <div className="text-sm text-muted-foreground">加载中...</div>}
        {error && (
          <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            <AlertCircle className="mr-1 inline h-3 w-3" />
            {(error as TauriError).message}
          </div>
        )}
        {data && data.length === 0 && !isLoading && (
          <div className="space-y-4">
            <EmptyState
              icon={Puzzle}
              title="还没有插件"
              description="把含 manifest.json 的目录放到 plugins 文件夹下,CmdFlow 会自动发现"
            />
            <PluginGuide />
          </div>
        )}
        {data && data.length > 0 && (
          <div className="space-y-2">
            {data.map((info: PluginInfo) => (
              <Card key={info.plugin.id}>
                <CardHeader className="flex flex-row items-center justify-between space-y-0">
                  <div className="space-y-1">
                    <CardTitle className="flex items-center gap-2 text-sm">
                      <Puzzle className="h-4 w-4" />
                      {info.plugin.name}
                      <Badge variant="outline" className="text-[10px]">
                        {info.plugin.format}
                      </Badge>
                      <span className="text-xs font-normal text-muted-foreground">
                        v{info.plugin.version}
                      </span>
                    </CardTitle>
                    {info.plugin.description && (
                      <CardDescription>{info.plugin.description}</CardDescription>
                    )}
                  </div>
                  <div className="flex items-center gap-1.5">
                    <Button
                      size="sm"
                      variant={info.plugin.enabled ? "default" : "outline"}
                      onClick={() =>
                        toggle.mutate({ id: info.plugin.id, enabled: !info.plugin.enabled })
                      }
                      disabled={toggle.isPending}
                    >
                      {info.plugin.enabled ? <Power className="h-3 w-3" /> : <PowerOff className="h-3 w-3" />}
                      {info.plugin.enabled ? "已启用" : "已禁用"}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setTesting(info)}
                      title="测试运行"
                    >
                      <Play className="h-3 w-3" />
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setDeletingId(info.plugin.id)}
                      title="卸载"
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                    {info.plugin.author && <span>by {info.plugin.author}</span>}
                    <span>·</span>
                    <span className="font-mono">{info.plugin.id}</span>
                    <span>·</span>
                    <span>{(info.size_bytes / 1024).toFixed(1)} KB</span>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>

      {/* 删除确认 */}
      <Dialog
        open={!!deletingId}
        onOpenChange={(o) => !o && setDeletingId(null)}
        title="卸载插件?"
        description="会从 plugins 目录中删除插件文件夹,无法恢复。"
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeletingId(null)}>取消</Button>
            <Button
              variant="destructive"
              onClick={() => deletingId && remove.mutate(deletingId)}
              disabled={remove.isPending}
            >
              {remove.isPending ? "卸载中..." : "确认卸载"}
            </Button>
          </>
        }
      >
        <p className="text-sm">插件目录: <code className="text-xs">{deletingId}</code></p>
      </Dialog>

      {/* 测试运行 */}
      <TestPluginDialog
        plugin={testing}
        onClose={() => setTesting(null)}
      />
    </div>
  );
}

function TestPluginDialog({
  plugin,
  onClose,
}: {
  plugin: PluginInfo | null;
  onClose: () => void;
}) {
  const [functionName, setFunctionName] = useState("run");
  const [argsJson, setArgsJson] = useState("{}");
  const [output, setOutput] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = useMutation({
    mutationFn: async () => {
      if (!plugin) return null;
      let args: unknown = {};
      try {
        args = JSON.parse(argsJson);
      } catch (e) {
        throw new Error(`参数 JSON 解析失败: ${(e as Error).message}`);
      }
      if (plugin.plugin.format === "wasm") {
        return api.plugin.executeWasm(plugin.plugin.id, functionName, args);
      } else {
        return api.plugin.execute(plugin.plugin.id, functionName, args);
      }
    },
    onSuccess: (data) => {
      setOutput(JSON.stringify(data, null, 2));
      setError(null);
    },
    onError: (e) => {
      setError((e as Error).message);
      setOutput(null);
    },
  });

  return (
    <Dialog
      open={!!plugin}
      onOpenChange={(o) => !o && onClose()}
      title={plugin ? `测试: ${plugin.plugin.name}` : ""}
      description={`${plugin?.plugin.format ?? ""} 插件 · ${plugin?.plugin.id ?? ""}`}
      maxWidth="2xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>关闭</Button>
          <Button
            onClick={() => run.mutate()}
            disabled={run.isPending || !plugin}
          >
            {run.isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                运行中...
              </>
            ) : (
              <>
                <Play className="h-4 w-4" />
                运行
              </>
            )}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">函数名</label>
          <Input
            value={functionName}
            onChange={(e) => setFunctionName(e.target.value)}
            placeholder="run"
          />
        </div>
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">参数 (JSON)</label>
          <textarea
            value={argsJson}
            onChange={(e) => setArgsJson(e.target.value)}
            className="w-full rounded-md border bg-background px-3 py-2 font-mono text-xs h-24 resize-y"
            spellCheck={false}
          />
        </div>
        {(output || error) && (
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">输出</label>
            <pre
              className={`rounded-md border p-3 font-mono text-xs overflow-auto max-h-64 ${
                error
                  ? "border-destructive bg-destructive/5 text-destructive"
                  : "border-emerald-500/50 bg-emerald-500/5"
              }`}
            >
{error ?? output}
            </pre>
          </div>
        )}
      </div>
    </Dialog>
  );
}

function PluginGuide() {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">插件开发快速入门</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3 text-xs text-muted-foreground">
        <div>
          <div className="mb-1 font-medium text-foreground">目录结构</div>
          <pre className="rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
{`plugins/
  hello-js/
    manifest.json
    index.js
  hello-wasm/
    manifest.json
    plugin.wasm`}
          </pre>
        </div>
        <div>
          <div className="mb-1 font-medium text-foreground">manifest.json (JS)</div>
          <pre className="rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
{`{
  "id": "hello-js",
  "name": "Hello (JS)",
  "version": "1.0.0",
  "format": "js",
  "entry": "index.js"
}`}
          </pre>
        </div>
        <div>
          <div className="mb-1 font-medium text-foreground">index.js</div>
          <pre className="rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
{`exports.run = function(args) {
  return { greeting: "Hello " + (args.name || "world") };
};`}
          </pre>
        </div>

        <div className="mt-4 border-t pt-3">
          <div className="mb-1 font-medium text-foreground">WASM 插件合约 (Phase 9.3)</div>
          <div className="mb-2">.wasm 必须导出以下符号 (Rust/AssemblyScript/Zig 通用):</div>
          <pre className="rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
{`(module
  (memory (export "memory") 1)
  (func (export "alloc") (param $sz i32) (result i32) ...)
  (func (export "dealloc") (param $p i32) (param $sz i32) ...)
  (func (export "run")
    (param $fptr i32) (param $flen i32)   ;; 函数名 JSON
    (param $iptr i32) (param $ilen i32)   ;; 入参 JSON
    (result i64)                          ;; 高32=out_ptr, 低32=out_len
  ))`}
          </pre>
          <div className="mt-2">
            流程: 1) 读 func_name 找到目标函数 2) 读 input JSON 解析
            3) 调用对应函数 4) 序列化为 JSON 写回线性内存
            5) 返回 (out_ptr &lt;&lt; 32) | out_len
          </div>
        </div>
        <div className="rounded-md border border-blue-500/50 bg-blue-500/10 p-2 text-blue-600">
          <AlertCircle className="mr-1 inline h-3 w-3" />
          Phase 9.3: WASM 插件已可执行 (wasmtime 24.x),带 10s 超时
        </div>
      </CardContent>
    </Card>
  );
}
