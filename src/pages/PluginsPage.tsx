import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Puzzle, Power, PowerOff, Trash2, RefreshCw, AlertCircle,
} from "lucide-react";
import { api, type PluginInfo, TauriError } from "@/lib/tauri";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/Badge";
import { EmptyState } from "@/components/ui/EmptyState";
import { Dialog } from "@/components/ui/Dialog";

export function PluginsPage() {
  const qc = useQueryClient();
  const [deletingId, setDeletingId] = useState<string | null>(null);

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
    </div>
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
  hello-world/
    manifest.json
    index.js`}
          </pre>
        </div>
        <div>
          <div className="mb-1 font-medium text-foreground">manifest.json</div>
          <pre className="rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
{`{
  "id": "hello-world",
  "name": "Hello World",
  "version": "1.0.0",
  "author": "Your Name",
  "description": "示例插件",
  "format": "js",
  "entry": "index.js",
  "permissions": ["fs.read"]
}`}
          </pre>
        </div>
        <div>
          <div className="mb-1 font-medium text-foreground">index.js (JS 插件)</div>
          <pre className="rounded bg-slate-900 p-2 font-mono text-[10px] text-slate-100">
{`// Phase 8: QuickJS 嵌入后支持完整 JS
// Phase 7: 占位,主流程可发现/启用/禁用
exports.run = function() {
  return "Hello from plugin";
};`}
          </pre>
        </div>
        <div className="rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-amber-600">
          <AlertCircle className="mr-1 inline h-3 w-3" />
          Phase 7: JS 引擎 (QuickJS/Boa) 还在集成中,目前只支持扫描+元数据展示
        </div>
      </CardContent>
    </Card>
  );
}
