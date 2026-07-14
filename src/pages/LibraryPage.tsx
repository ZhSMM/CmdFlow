import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api, TauriError, type Command } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Library as LibraryIcon, Plus, Pencil, Trash2, Play, Star } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { CommandEditor } from "@/components/forms/CommandEditor";
import { Dialog } from "@/components/ui/Dialog";
import { useNavigate } from "react-router-dom";
import { formatDate } from "@/lib/utils";
import { Input } from "@/components/ui/Input";

export function LibraryPage() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const [editorOpen, setEditorOpen] = useState(false);
  const [editId, setEditId] = useState<string | undefined>(undefined);
  const [search, setSearch] = useState("");
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["commands", { search }],
    queryFn: () => api.library.list({ search: search || undefined }),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.library.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["commands"] });
      setDeletingId(null);
    },
  });

  const favs = useQuery({ queryKey: ["favorites"], queryFn: () => api.favorite.list() });
  const favSet = new Set((favs.data ?? []).map((f) => f.command_id));
  const toggleFav = useMutation({
    mutationFn: (id: string) =>
      favSet.has(id) ? api.favorite.remove(id) : api.favorite.add(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["favorites"] }),
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="text-lg font-semibold">命令库</h1>
          <p className="text-xs text-muted-foreground">
            管理可复用的命令模板，每个命令可声明参数
          </p>
        </div>
        <Button
          onClick={() => {
            setEditId(undefined);
            setEditorOpen(true);
          }}
        >
          <Plus className="h-4 w-4" />
          新建命令
        </Button>
      </div>

      <div className="border-b px-6 py-2">
        <Input
          placeholder="搜索命令名称或描述..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="max-w-md"
        />
      </div>

      <div className="flex-1 overflow-auto p-6">
        {isLoading && <div className="text-sm text-muted-foreground">加载中...</div>}
        {error && (
          <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            加载失败：{(error as TauriError).message}
          </div>
        )}
        {data && data.length === 0 && (
          <EmptyState
            icon={LibraryIcon}
            title={search ? "没有匹配的命令" : "命令库还是空的"}
            description={search ? "试试别的关键词" : "点「新建命令」开始"}
            action={
              !search && (
                <Button
                  onClick={() => {
                    setEditId(undefined);
                    setEditorOpen(true);
                  }}
                >
                  <Plus className="h-4 w-4" />
                  新建命令
                </Button>
              )
            }
          />
        )}
        {data && data.length > 0 && (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
            {data.map((cmd: Command) => (
              <Card key={cmd.id} className="group relative">
                <CardHeader>
                  <CardTitle className="flex items-center gap-2 pr-20">
                    <span className="truncate">{cmd.name}</span>
                  </CardTitle>
                  {cmd.description && (
                    <CardDescription className="line-clamp-2">
                      {cmd.description}
                    </CardDescription>
                  )}
                </CardHeader>
                <CardContent>
                  <div className="flex items-center justify-between text-xs text-muted-foreground">
                    <span className="rounded bg-secondary px-1.5 py-0.5">
                      {cmd.type}
                    </span>
                    <span>
                      v{cmd.current_ver} · {formatDate(cmd.updated_at)}
                    </span>
                  </div>
                  {cmd.tags.length > 0 && (
                    <div className="mt-2 flex flex-wrap gap-1">
                      {cmd.tags.map((t) => (
                        <span
                          key={t}
                          className="rounded bg-secondary px-1.5 py-0.5 text-[10px]"
                        >
                          {t}
                        </span>
                      ))}
                    </div>
                  )}
                </CardContent>
                <div className="absolute right-2 top-2 flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleFav.mutate(cmd.id);
                    }}
                    className={
                      favSet.has(cmd.id)
                        ? "rounded p-1 text-amber-500"
                        : "rounded p-1 text-muted-foreground hover:bg-accent hover:text-amber-500"
                    }
                    title={favSet.has(cmd.id) ? "取消收藏" : "收藏"}
                  >
                    <Star
                      className="h-3.5 w-3.5"
                      fill={favSet.has(cmd.id) ? "currentColor" : "none"}
                      stroke={undefined}
                    />
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      nav(`/runner?cmd=${cmd.id}`);
                    }}
                    className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                    title="执行"
                  >
                    <Play className="h-3.5 w-3.5" />
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setEditId(cmd.id);
                      setEditorOpen(true);
                    }}
                    className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                    title="编辑"
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setDeletingId(cmd.id);
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

      <CommandEditor
        open={editorOpen}
        onOpenChange={setEditorOpen}
        commandId={editId}
      />

      <Dialog
        open={!!deletingId}
        onOpenChange={(o) => !o && setDeletingId(null)}
        title="确认删除"
        description="删除后无法恢复，命令库和历史记录都会清理。"
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
        <p className="text-sm">
          确定要删除这个命令吗？相关的执行历史会保留但失去引用。
        </p>
      </Dialog>
    </div>
  );
}
