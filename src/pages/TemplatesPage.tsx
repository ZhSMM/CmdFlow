import { useState, useMemo } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  LayoutGrid,
  Download,
  Search,
  X,
  Eye,
  GitBranch,
  Container,
  Trash2,
  Database,
  Archive,
  Activity,
  Sparkles,
  Loader2,
  CheckCircle2,
} from "lucide-react";
import { api, type TemplateSummary, TauriError } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { cn } from "@/lib/utils";

const ICON_MAP: Record<string, React.ComponentType<{ className?: string }>> = {
  GitBranch,
  Container,
  Trash2,
  Database,
  Archive,
  Activity,
};

function TemplateIcon({ name, className }: { name: string; className?: string }) {
  const Icon = ICON_MAP[name] ?? LayoutGrid;
  return <Icon className={className} />;
}

export function TemplatesPage() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const [keyword, setKeyword] = useState("");
  const [activeCategory, setActiveCategory] = useState<string>("全部");
  const [previewing, setPreviewing] = useState<TemplateSummary | null>(null);
  const [importing, setImporting] = useState<string | null>(null);
  const [justImported, setJustImported] = useState<string | null>(null);
  const [importName, setImportName] = useState("");

  const { data: templates, isLoading, error } = useQuery({
    queryKey: ["templates"],
    queryFn: () => api.template.list(),
  });

  const importTemplate = useMutation({
    mutationFn: ({ id, name }: { id: string; name?: string }) =>
      api.template.import(id, name),
    onSuccess: (res, vars) => {
      qc.invalidateQueries({ queryKey: ["workflows"] });
      setImporting(null);
      setImportName("");
      setJustImported(vars.id);
      // 1.5s 后跳到编辑器
      setTimeout(() => {
        setJustImported(null);
        nav(`/workflows/${res.workflow_id}`);
      }, 700);
    },
    onError: (e) => {
      alert(`导入失败: ${(e as TauriError).message}`);
      setImporting(null);
    },
  });

  const categories = useMemo(() => {
    if (!templates) return ["全部"];
    return ["全部", ...Array.from(new Set(templates.map((t) => t.category)))];
  }, [templates]);

  const filtered = useMemo(() => {
    if (!templates) return [];
    return templates.filter((t) => {
      if (activeCategory !== "全部" && t.category !== activeCategory) return false;
      if (keyword.trim()) {
        const k = keyword.toLowerCase();
        return (
          t.name.toLowerCase().includes(k) ||
          t.description.toLowerCase().includes(k) ||
          t.tags.some((tag) => tag.toLowerCase().includes(k))
        );
      }
      return true;
    });
  }, [templates, keyword, activeCategory]);

  const handleImport = (t: TemplateSummary) => {
    setImporting(t.id);
    setImportName(t.name);
  };

  const confirmImport = () => {
    if (!importing) return;
    importTemplate.mutate({
      id: importing,
      name: importName.trim() || undefined,
    });
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b px-6 py-3">
        <div>
          <h1 className="flex items-center gap-2 text-lg font-semibold">
            <LayoutGrid className="h-5 w-5" />
            模板市场
          </h1>
          <p className="text-xs text-muted-foreground">
            一键导入内置工作流模板,导入后可在工作流页面编辑
          </p>
        </div>
        <div className="relative w-64">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            placeholder="搜索模板..."
            className="pl-8 pr-8"
          />
          {keyword && (
            <button
              onClick={() => setKeyword("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* Category filter */}
      <div className="flex items-center gap-1.5 overflow-x-auto border-b bg-card/30 px-6 py-2">
        {categories.map((c) => (
          <button
            key={c}
            onClick={() => setActiveCategory(c)}
            className={cn(
              "rounded-full px-3 py-1 text-xs whitespace-nowrap transition-colors",
              activeCategory === c
                ? "bg-primary text-primary-foreground"
                : "bg-secondary text-secondary-foreground hover:bg-secondary/70",
            )}
          >
            {c}
          </button>
        ))}
        <div className="ml-auto text-xs text-muted-foreground">
          {filtered.length} / {templates?.length ?? 0} 个模板
        </div>
      </div>

      {/* Grid */}
      <div className="flex-1 overflow-auto p-6">
        {isLoading && (
          <div className="flex items-center justify-center py-12 text-sm text-muted-foreground">
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            加载模板中...
          </div>
        )}
        {error && (
          <div className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            {(error as TauriError).message}
          </div>
        )}
        {templates && templates.length === 0 && (
          <EmptyState
            icon={LayoutGrid}
            title="还没有模板"
            description="内置模板加载失败,请检查 src-tauri/templates/ 目录"
          />
        )}
        {filtered.length === 0 && templates && templates.length > 0 && (
          <EmptyState
            icon={Search}
            title="没有匹配的模板"
            description={`没有找到含「${keyword}」的模板`}
          />
        )}
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
          {filtered.map((t) => (
            <TemplateCard
              key={t.id}
              template={t}
              importing={importTemplate.isPending && importing === t.id}
              justImported={justImported === t.id}
              onPreview={() => setPreviewing(t)}
              onImport={() => handleImport(t)}
            />
          ))}
        </div>
      </div>

      {/* Preview dialog */}
      <TemplatePreviewDialog
        template={previewing}
        onClose={() => setPreviewing(null)}
        onImport={(t) => {
          setPreviewing(null);
          handleImport(t);
        }}
      />

      {/* Import confirm dialog */}
      <Dialog
        open={!!importing}
        onOpenChange={(o) => {
          if (!o) {
            setImporting(null);
            setImportName("");
          }
        }}
        title="导入模板"
        description={
          templates?.find((t) => t.id === importing)?.name ?? ""
        }
        footer={
          <>
            <Button
              variant="ghost"
              onClick={() => {
                setImporting(null);
                setImportName("");
              }}
              disabled={importTemplate.isPending}
            >
              取消
            </Button>
            <Button onClick={confirmImport} disabled={importTemplate.isPending}>
              {importTemplate.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  导入中...
                </>
              ) : (
                <>
                  <Download className="h-4 w-4" />
                  导入
                </>
              )}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">
            模板会复制一份到你的工作流列表,导入后可以自由修改。
          </p>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              工作流名称(留空使用模板默认名)
            </label>
            <Input
              autoFocus
              value={importName}
              onChange={(e) => setImportName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !importTemplate.isPending) {
                  confirmImport();
                }
              }}
            />
          </div>
        </div>
      </Dialog>
    </div>
  );
}

function TemplateCard({
  template,
  importing,
  justImported,
  onPreview,
  onImport,
}: {
  template: TemplateSummary;
  importing: boolean;
  justImported: boolean;
  onPreview: () => void;
  onImport: () => void;
}) {
  return (
    <Card className="group flex flex-col hover:bg-accent/20 transition-colors">
      <CardHeader className="pb-2">
        <div className="flex items-start gap-3">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
            <TemplateIcon name={template.icon} className="h-5 w-5" />
          </div>
          <div className="min-w-0 flex-1">
            <CardTitle className="truncate text-sm">{template.name}</CardTitle>
            <div className="mt-0.5 flex items-center gap-1.5 text-[10px] text-muted-foreground">
              <span className="rounded bg-secondary px-1.5 py-0.5">
                {template.category}
              </span>
              <span>{template.node_count} 节点</span>
              <span>·</span>
              <span>{template.edge_count} 连线</span>
            </div>
          </div>
          {template.builtin && (
            <Sparkles className="h-3.5 w-3.5 shrink-0 text-amber-500/70" />
          )}
        </div>
      </CardHeader>
      <CardContent className="flex-1 pb-3">
        <CardDescription className="line-clamp-2 min-h-[2.5rem] text-xs">
          {template.description}
        </CardDescription>
        {template.tags.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            {template.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground"
              >
                #{tag}
              </span>
            ))}
          </div>
        )}
      </CardContent>
      <div className="flex items-center gap-1 border-t bg-card/50 px-3 py-2">
        <Button
          size="sm"
          variant="ghost"
          className="h-7 text-xs"
          onClick={onPreview}
          disabled={importing}
        >
          <Eye className="h-3 w-3" />
          预览
        </Button>
        <div className="flex-1" />
        {justImported ? (
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs text-emerald-600"
            disabled
          >
            <CheckCircle2 className="h-3 w-3" />
            已导入
          </Button>
        ) : (
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={onImport}
            disabled={importing}
          >
            {importing ? (
              <>
                <Loader2 className="h-3 w-3 animate-spin" />
                导入中
              </>
            ) : (
              <>
                <Download className="h-3 w-3" />
                导入
              </>
            )}
          </Button>
        )}
      </div>
    </Card>
  );
}

function TemplatePreviewDialog({
  template,
  onClose,
  onImport,
}: {
  template: TemplateSummary | null;
  onClose: () => void;
  onImport: (t: TemplateSummary) => void;
}) {
  const { data: yaml, isLoading } = useQuery({
    queryKey: ["template-yaml", template?.id],
    queryFn: () => (template ? api.template.get(template.id) : Promise.resolve("")),
    enabled: !!template,
  });

  return (
    <Dialog
      open={!!template}
      onOpenChange={(o) => !o && onClose()}
      title={template ? `预览: ${template.name}` : ""}
      description={template?.description}
      maxWidth="2xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            关闭
          </Button>
          <Button
            onClick={() => template && onImport(template)}
            disabled={!template}
          >
            <Download className="h-4 w-4" />
            导入此模板
          </Button>
        </>
      }
    >
      {template && (
        <div className="space-y-3">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span className="rounded bg-secondary px-2 py-0.5">
              {template.category}
            </span>
            {template.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-full bg-muted px-2 py-0.5 text-muted-foreground"
              >
                #{tag}
              </span>
            ))}
            <span className="text-muted-foreground">
              {template.node_count} 节点 / {template.edge_count} 连线
            </span>
          </div>
          <div className="rounded-md border bg-muted/30 p-3 font-mono text-xs leading-relaxed max-h-96 overflow-auto">
            {isLoading ? (
              <div className="flex items-center gap-2 text-muted-foreground">
                <Loader2 className="h-3 w-3 animate-spin" />
                加载 YAML...
              </div>
            ) : (
              <pre className="whitespace-pre-wrap break-all">{yaml}</pre>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}
