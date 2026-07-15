import { useEffect, useState } from "react";
import type {
  Connection,
  Edge,
  Node,
  NodeChange,
  EdgeChange,
  ReactFlowProps,
} from "reactflow";

interface ReactFlowLazyProps {
  nodes: Node[];
  edges: Edge[];
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  onNodeClick: ReactFlowProps["onNodeClick"];
  onEdgeClick: ReactFlowProps["onEdgeClick"];
  onPaneClick: () => void;
  /** 显式启用节点拖动 (ReactFlow 11 默认 true, 但作为防御性配置) */
  nodesDraggable?: boolean;
  /** 显式启用节点选中 */
  elementsSelectable?: boolean;
}

/**
 * ReactFlow 的懒加载包装
 * reactflow 体积大 (~170KB),只在进入工作流编辑时才加载。
 */
export function ReactFlowLazy(props: ReactFlowLazyProps) {
  const [mod, setMod] = useState<typeof import("reactflow") | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      import("reactflow"),
      import("reactflow/dist/style.css"),
    ]).then(([m]) => {
      if (!cancelled) setMod(m);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!mod) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        加载画布...
      </div>
    );
  }

  const RF = mod.default;
  const { Background, Controls, MiniMap } = mod;

  return (
    <RF
      nodes={props.nodes}
      edges={props.edges}
      onNodesChange={props.onNodesChange}
      onEdgesChange={props.onEdgesChange}
      onConnect={props.onConnect}
      onNodeClick={props.onNodeClick}
      onEdgeClick={props.onEdgeClick}
      onPaneClick={props.onPaneClick}
      nodesDraggable={props.nodesDraggable ?? true}
      elementsSelectable={props.elementsSelectable ?? true}
      fitView
      deleteKeyCode={["Delete", "Backspace"]}
    >
      <Background />
      <Controls />
      <MiniMap pannable zoomable />
    </RF>
  );
}
