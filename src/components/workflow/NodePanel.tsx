import { useState } from "react";
import { Search, Terminal, Globe, FileText, GitBranch, Repeat, Clock, Box, Sparkles } from "lucide-react";
import { Input } from "@/components/ui/Input";
import { cn } from "@/lib/utils";

interface NodeTypeMeta {
  type_id: string;
  display_name: string;
  category: string;
  description: string;
}

const ICONS: Record<string, any> = {
  cmd: Terminal,
  script: Terminal,
  http: Globe,
  file: FileText,
  condition: GitBranch,
  loop: Repeat,
  delay: Clock,
  subworkflow: Box,
  ai: Sparkles,
};

const CATEGORIES = [
  { key: "core", label: "核心" },
  { key: "io", label: "IO" },
  { key: "control", label: "控制" },
];

export function NodePanel({ nodeTypes }: { nodeTypes: NodeTypeMeta[] }) {
  const [search, setSearch] = useState("");

  const filtered = nodeTypes.filter(
    (t) =>
      !search ||
      t.display_name.includes(search) ||
      t.type_id.includes(search) ||
      t.description.includes(search),
  );

  const onDragStart = (event: React.DragEvent, type: NodeTypeMeta) => {
    event.dataTransfer.setData(
      "application/reactflow",
      JSON.stringify({ type_id: type.type_id }),
    );
    event.dataTransfer.effectAllowed = "move";
  };

  return (
    <div className="w-56 flex-shrink-0 border-r bg-card">
      <div className="border-b p-2">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索节点..."
            className="h-8 pl-7 text-xs"
          />
        </div>
      </div>

      <div className="overflow-auto" style={{ maxHeight: "calc(100vh - 200px)" }}>
        {CATEGORIES.map((cat) => {
          const items = filtered.filter((t) => t.category === cat.key);
          if (items.length === 0) return null;
          return (
            <div key={cat.key} className="p-2">
              <div className="mb-1.5 px-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                {cat.label}
              </div>
              <div className="space-y-1">
                {items.map((t) => {
                  const Icon = ICONS[t.type_id] || Box;
                  return (
                    <div
                      key={t.type_id}
                      draggable
                      onDragStart={(e) => onDragStart(e, t)}
                      className={cn(
                        "flex cursor-grab items-center gap-2 rounded-md border bg-background p-2 text-xs",
                        "hover:border-primary/50 hover:bg-accent/50 active:cursor-grabbing",
                      )}
                      title={t.description}
                    >
                      <Icon className="h-3.5 w-3.5 flex-shrink-0 text-primary" />
                      <span className="flex-1 truncate">{t.display_name}</span>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
