import { useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  Library as LibraryIcon, Plus, Pencil, Trash2, Play, Star,
  Folder, FolderOpen, ChevronRight, ChevronDown, Search,
} from "lucide-react";
import { api, TauriError, type CategoryNode } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { CommandEditor } from "@/components/forms/CommandEditor";
import { Dialog } from "@/components/ui/Dialog";
import { formatDate, cn } from "@/lib/utils";

export function LibraryPage() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const [editorOpen, setEditorOpen] = useState(false);
  const [editId, setEditId] = useState<string | undefined>(undefined);
  const [search, setSearch] = useState("");
  const [deletingCommand, setDeletingCommand] = useState<string | null>(null);
  const [deletingCategory, setDeletingCategory] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set(["__root__"]));
  const [selectedNode, setSelectedNode] = useState<string>("__root__");

  const { data: tree, isLoading: treeLoading, error: treeError } = useQuery({
    queryKey: ["category-tree"],
    queryFn: () => api.category.tree(),
  });
  const { data: favs } = useQuery({
    queryKey: ["favorites"],
    queryFn: () => api.favorite.list(),
  });
  const favSet = useMemo(() => new Set((favs ?? []).map((f) => f.command_id)), [favs]);

  const toggleFav = useMutation({
    mutationFn: (id: string) =>
      favSet.has(id) ? api.favorite.remove(id) : api.favorite.add(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["favorites"] }),
  });

  const removeCommand = useMutation({
    mutationFn: (id: string) => api.library.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["category-tree"] });
      qc.invalidateQueries({ queryKey: ["commands"] });
      setDeletingCommand(null);
    },
  });

  const removeCategory = useMutation({
    mutationFn: (id: string) => api.category.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["category-tree"] });
      setDeletingCategory(null);
      if (selectedNode === deletingCategory) setSelectedNode("__root__");
    },
  });

  const moveCommand = useMutation({
    mutationFn: ({ id, cat }: { id: string; cat: string | null }) =>
      api.category.moveCommand(id, cat),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["category-tree"] }),
  });

  // 找到当前选中节点的命令列表
  const currentCommands = useMemo(() => {
    if (!tree) return [];
    const findNode = (nodes: CategoryNode[]): CategoryNode | null => {
      for (const n of nodes) {
        if (n.id === selectedNode) return n;
        const sub = findNode(n.subcategories);
        if (sub) return sub;
      }
      return null;
    };
    return findNode(tree)?.commands ?? [];
  }, [tree, selectedNode]);

  // 搜索过滤
  const filteredCommands = useMemo(() => {
    if (!search) return currentCommands;
    const s = search.toLowerCase();
    return currentCommands.filter(
      (c) => c.name.toLowerCase().includes(s) ||
             (c.description ?? "").toLowerCase().includes(s) ||
             c.tags.some((t) => t.toLowerCase().includes(s)),
    );
  }, [currentCommands, search]);

  const toggle = (id: string) => {
    setExpanded((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };

  return (
    <div className="flex h-full">
      {/* 左侧: 树形 */}
      <div className="flex w-64 flex-shrink-0 flex-col border-r bg-card">
        <div className="flex items-center justify-between border-b px-3 py-2">
          <div className="text-sm font-semibold">分类</div>
          <NewCategoryButton
            parentId={selectedNode === "__root__" ? null : selectedNode}
            onCreated={() => {
              qc.invalidateQueries({ queryKey: ["category-tree"] });
            }}
          />
        </div>
        <div className="flex-1 overflow-auto p-1">
          {treeLoading && <div className="p-2 text-xs text-muted-foreground">加载中...</div>}
          {treeError && (
            <div className="p-2 text-xs text-destructive">
              {(treeError as TauriError).message}
            </div>
          )}
          {tree && tree.length === 0 && (
            <div className="p-2 text-xs text-muted-foreground">还没有分类</div>
          )}
          {tree?.map((node) => (
            <CategoryTree
              key={node.id}
              node={node}
              depth={0}
              expanded={expanded}
              selected={selectedNode}
              onToggle={toggle}
              onSelect={setSelectedNode}
              onDelete={(id) => setDeletingCategory(id)}
            />
          ))}
        </div>
      </div>

      {/* 右侧: 命令列表 */}
      <div className="flex flex-1 flex-col">
        <div className="flex items-center gap-2 border-b px-6 py-3">
          <div>
            <h1 className="text-lg font-semibold">命令库</h1>
            <p className="text-xs text-muted-foreground">
              {currentCommands.length} 个命令 · 点 ★ 收藏 · 点 ▶ 执行
            </p>
          </div>
          <div className="ml-auto flex items-center gap-2">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="搜索当前分类..."
                className="h-8 w-56 pl-7 text-xs"
              />
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
        </div>

        <div className="flex-1 overflow-auto p-6">
          {filteredCommands.length === 0 && (
            <EmptyState
              icon={LibraryIcon}
              title={search ? "没找到匹配的命令" : "这个分类是空的"}
              description={search ? `搜索 "${search}" 没有结果` : "点「新建命令」添加第一个"}
            />
          )}
          {filteredCommands.length > 0 && (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
              {filteredCommands.map((cmd) => (
                <Card key={cmd.id} className="group relative">
                  <CardHeader>
                    <CardTitle className="flex items-center gap-2 pr-16">
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
                    <MoveMenu
                      commandId={cmd.id}
                      currentCategoryId={cmd.category}
                      tree={tree ?? []}
                      onMove={(cat) => moveCommand.mutate({ id: cmd.id, cat })}
                    />
                  </CardContent>
                  <div className="absolute right-2 top-2 flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                    <button
                      onClick={() => toggleFav.mutate(cmd.id)}
                      className={cn(
                        "rounded p-1",
                        favSet.has(cmd.id) ? "text-amber-500" : "text-muted-foreground hover:bg-accent hover:text-amber-500",
                      )}
                      title={favSet.has(cmd.id) ? "取消收藏" : "收藏"}
                    >
                      <Star className="h-3.5 w-3.5" fill={favSet.has(cmd.id) ? "currentColor" : "none"} />
                    </button>
                    <button
                      onClick={() => nav(`/runner?cmd=${cmd.id}`)}
                      className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                      title="执行"
                    >
                      <Play className="h-3.5 w-3.5" />
                    </button>
                    <button
                      onClick={() => {
                        setEditId(cmd.id);
                        setEditorOpen(true);
                      }}
                      className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                      title="编辑"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      onClick={() => setDeletingCommand(cmd.id)}
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
      </div>

      <CommandEditor
        open={editorOpen}
        onOpenChange={setEditorOpen}
        commandId={editId}
        onSaved={() => qc.invalidateQueries({ queryKey: ["category-tree"] })}
      />

      <Dialog
        open={!!deletingCommand}
        onOpenChange={(o) => !o && setDeletingCommand(null)}
        title="删除命令?"
        description="删除后无法恢复"
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeletingCommand(null)}>取消</Button>
            <Button
              variant="destructive"
              disabled={removeCommand.isPending}
              onClick={() => deletingCommand && removeCommand.mutate(deletingCommand)}
            >
              {removeCommand.isPending ? "删除中..." : "确认删除"}
            </Button>
          </>
        }
      >
        <p className="text-sm">相关的执行历史会保留但失去引用。</p>
      </Dialog>

      <Dialog
        open={!!deletingCategory}
        onOpenChange={(o) => !o && setDeletingCategory(null)}
        title="删除分类?"
        description="子分类会被一并删除,分类下的命令会归入根目录"
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeletingCategory(null)}>取消</Button>
            <Button
              variant="destructive"
              disabled={removeCategory.isPending}
              onClick={() => deletingCategory && removeCategory.mutate(deletingCategory)}
            >
              {removeCategory.isPending ? "删除中..." : "确认删除"}
            </Button>
          </>
        }
      >
        <p className="text-sm">分类 ID: <code className="text-xs">{deletingCategory}</code></p>
      </Dialog>
    </div>
  );
}

// ==================== 树节点组件 ====================

function CategoryTree({
  node, depth, expanded, selected, onToggle, onSelect, onDelete,
}: {
  node: CategoryNode;
  depth: number;
  expanded: Set<string>;
  selected: string;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const hasChildren = node.subcategories.length > 0 || node.commands.length > 0;
  const isOpen = expanded.has(node.id);
  const isSelected = selected === node.id;

  return (
    <div>
      <div
        className={cn(
          "group flex items-center gap-1 rounded-md px-1.5 py-1 text-sm cursor-pointer",
          isSelected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50",
        )}
        style={{ paddingLeft: 4 + depth * 12 }}
        onClick={() => onSelect(node.id)}
      >
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (hasChildren) onToggle(node.id);
          }}
          className="h-4 w-4 shrink-0 text-muted-foreground"
        >
          {hasChildren ? (
            isOpen ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />
          ) : (
            <span className="inline-block h-3 w-3" />
          )}
        </button>
        {isOpen ? (
          <FolderOpen className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        ) : (
          <Folder className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        )}
        <span className="flex-1 truncate text-xs">{node.name}</span>
        <span className="rounded bg-secondary px-1 text-[10px] text-muted-foreground">
          {node.commands.length}
        </span>
        {node.id !== "__root__" && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDelete(node.id);
            }}
            className="h-4 w-4 shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-destructive"
            title="删除分类"
          >
            <Trash2 className="h-3 w-3" />
          </button>
        )}
      </div>
      {isOpen && (
        <div>
          {node.subcategories.map((sub) => (
            <CategoryTree
              key={sub.id}
              node={sub}
              depth={depth + 1}
              expanded={expanded}
              selected={selected}
              onToggle={onToggle}
              onSelect={onSelect}
              onDelete={onDelete}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function NewCategoryButton({
  parentId, onCreated,
}: {
  parentId: string | null;
  onCreated: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const create = useMutation({
    mutationFn: () => api.category.create({ name, parent_id: parentId ?? undefined }),
    onSuccess: () => {
      setOpen(false);
      setName("");
      onCreated();
    },
  });
  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title={parentId ? "新建子分类" : "新建分类"}
      className="max-w-sm"
      footer={
        <>
          <Button variant="ghost" onClick={() => setOpen(false)}>取消</Button>
          <Button
            onClick={() => name.trim() && create.mutate()}
            disabled={!name.trim() || create.isPending}
          >
            创建
          </Button>
        </>
      }
    >
      <Input
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="分类名"
        onKeyDown={(e) => {
          if (e.key === "Enter" && name.trim()) create.mutate();
        }}
      />
      {create.error && (
        <div className="mt-2 text-xs text-destructive">
          {(create.error as TauriError).message}
        </div>
      )}
    </Dialog>
  );
}

function MoveMenu({
  commandId, currentCategoryId, tree, onMove,
}: {
  commandId: string;
  currentCategoryId: string | null;
  tree: CategoryNode[];
  onMove: (cat: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative mt-1.5">
      <button
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
        className="flex w-full items-center gap-1 rounded-md border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-accent/30"
      >
        <span className="truncate">
          {currentCategoryId ? `📁 ${findPath(tree, currentCategoryId) ?? "?"}` : "📦 未分类"}
        </span>
        <span className="ml-auto">↕</span>
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-1 max-h-48 w-56 overflow-auto rounded-md border bg-popover p-1 text-xs shadow-lg">
          <button
            onClick={() => { onMove(null); setOpen(false); }}
            className={cn(
              "flex w-full items-center rounded-sm px-2 py-1 hover:bg-accent",
              currentCategoryId === null && "bg-accent",
            )}
          >
            📦 根目录 (未分类)
          </button>
          {tree.filter((n) => n.id !== "__root__").map((n) => (
            <CategoryOption
              key={n.id}
              node={n}
              currentId={currentCategoryId}
              commandId={commandId}
              onSelect={(id) => { onMove(id); setOpen(false); }}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function CategoryOption({
  node, currentId, commandId, onSelect, depth = 0,
}: {
  node: CategoryNode;
  currentId: string | null;
  commandId: string;
  onSelect: (id: string) => void;
  depth?: number;
}) {
  return (
    <>
      <button
        onClick={() => onSelect(node.id)}
        className={cn(
          "flex w-full items-center gap-1 rounded-sm px-2 py-1 hover:bg-accent",
          currentId === node.id && "bg-accent",
        )}
        style={{ paddingLeft: 8 + depth * 12 }}
      >
        <Folder className="h-3 w-3 text-amber-500" />
        <span className="truncate">{node.name}</span>
      </button>
      {node.subcategories.map((sub) => (
        <CategoryOption
          key={sub.id}
          node={sub}
          currentId={currentId}
          commandId={commandId}
          onSelect={onSelect}
          depth={depth + 1}
        />
      ))}
    </>
  );
}

function findPath(tree: CategoryNode[], id: string, path: string[] = []): string | null {
  for (const n of tree) {
    if (n.id === id) return [...path, n.name].join(" / ");
    const sub = findPath(n.subcategories, id, [...path, n.name]);
    if (sub) return sub;
  }
  return null;
}

function EmptyState({
  icon: Icon, title, description, action,
}: {
  icon: any;
  title: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <Icon className="h-12 w-12 text-muted-foreground/50" />
      <div>
        <h3 className="text-lg font-semibold">{title}</h3>
        {description && (
          <p className="mt-1 text-sm text-muted-foreground">{description}</p>
        )}
      </div>
      {action}
    </div>
  );
}
