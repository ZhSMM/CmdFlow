import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { api, type Workflow } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Workflow as WorkflowIcon, Plus, Trash2, Play } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Dialog } from "@/components/ui/Dialog";
import { formatDate } from "@/lib/utils";
import { WorkflowEditor } from "@/components/workflow/WorkflowEditor";
import { TauriError } from "@/lib/tauri";

export function WorkflowsPage() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const [showNew, setShowNew] = useState(false);
  const [newName, setNewName] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["workflows"],
    queryFn: () => api.workflow.list(),
  });

  const create = useMutation({
    mutationFn: (name: string) =>
      api.workflow.create({ name, trigger_type: "manual" }),
    onSuccess: (id) => {
      qc.invalidateQueries({ queryKey: ["workflows"] });
      setShowNew(false);
      setNewName("");
      setEditingId(id);
    },
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.workflow.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workflows"] });
      setDeletingId(null);
    },
  });

  // 选中工作流进入编辑模式
  if (editingId) {
    return <WorkflowEditorWrapper id={editingId} onBack={() => setEditingId(null)} />;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">工作流</h1>
          <p className="text-xs text-muted-foreground">DAG 工作流，画布可视化编排</p>
        </div>
        <Button onClick={() => setShowNew(true)}>
          <Plus className="h-4 w-4" />
          新建工作流
        </Button>
      </div>

      {showNew && (
        <div className="border-b bg-accent/30 p-4">
          <div className="flex items-center gap-2">
            <Input
              autoFocus
              placeholder="工作流名称"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && newName.trim()) create.mutate(newName.trim());
                if (e.key === "Escape") {
                  setShowNew(false);
                  setNewName("");
                }
              }}
            />
            <Button
              onClick={() => newName.trim() && create.mutate(newName.trim())}
              disabled={!newName.trim() || create.isPending}
            >
              创建
            </Button>
            <Button variant="ghost" onClick={() => setShowNew(false)}>
              取消
            </Button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-auto p-6">
        {isLoading && <div className="text-sm text-muted-foreground">加载中...</div>}
        {error && (
          <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            {(error as TauriError).message}
          </div>
        )}
        {data && data.length === 0 && (
          <EmptyState
            icon={WorkflowIcon}
            title="还没有工作流"
            description="点「新建工作流」开始"
            action={
              <Button onClick={() => setShowNew(true)}>
                <Plus className="h-4 w-4" />
                新建工作流
              </Button>
            }
          />
        )}
        {data && data.length > 0 && (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
            {data.map((wf: Workflow) => (
              <Card
                key={wf.id}
                className="group relative cursor-pointer hover:bg-accent/30"
                onClick={() => setEditingId(wf.id)}
              >
                <CardHeader>
                  <CardTitle className="flex items-center gap-2 pr-16">
                    <WorkflowIcon className="h-4 w-4" />
                    {wf.name}
                  </CardTitle>
                  {wf.description && (
                    <CardDescription className="line-clamp-2">
                      {wf.description}
                    </CardDescription>
                  )}
                </CardHeader>
                <CardContent>
                  <div className="flex items-center justify-between text-xs text-muted-foreground">
                    <span className="rounded bg-secondary px-1.5 py-0.5">
                      {wf.trigger_type}
                    </span>
                    <span>{formatDate(wf.updated_at)}</span>
                  </div>
                </CardContent>
                <div className="absolute right-2 top-2 flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      nav(`/runner?workflow=${wf.id}`);
                    }}
                    className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                    title="执行"
                  >
                    <Play className="h-3.5 w-3.5" />
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setDeletingId(wf.id);
                    }}
                    className="rounded p-1 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                    title="删除"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>

      <Dialog
        open={!!deletingId}
        onOpenChange={(o) => !o && setDeletingId(null)}
        title="确认删除工作流"
        description="删除后无法恢复。"
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeletingId(null)}>
              取消
            </Button>
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
        <p className="text-sm">工作流相关的执行历史会保留但失去引用。</p>
      </Dialog>
    </div>
  );
}

function WorkflowEditorWrapper({ id, onBack }: { id: string; onBack: () => void }) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b bg-card px-4 py-2">
        <Button size="sm" variant="ghost" onClick={onBack}>
          ← 返回列表
        </Button>
      </div>
      <div className="flex-1">
        <WorkflowEditor workflowId={id} />
      </div>
    </div>
  );
}
