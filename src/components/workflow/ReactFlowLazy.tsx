import { useEffect, useState } from "react";
import type {
  Connection,
  Edge,
  Node,
  NodeChange,
  EdgeChange,
  ReactFlowProps,
} from "@xyflow/react";
// CSS 静态 import: vite 会把它注入 <link> 标签, 避免动态 import 在 dev mode
// 下不生效 (症状: minimap 显示成三条横线、节点 cursor 不会变 grab)
// @xyflow/react 体积大, 但 CSS 不大, 静态 import 没问题
import "@xyflow/react/dist/style.css";

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
 * reactflow JS 体积大 (~170KB),只在进入工作流编辑时才加载。
 * CSS 必须静态 import, 否则 dev mode 下样式可能丢失。
 */
export function ReactFlowLazy(props: ReactFlowLazyProps) {
  const [mod, setMod] = useState<typeof import("@xyflow/react") | null>(null);

  useEffect(() => {
    let cancelled = false;
    import("@xyflow/react").then((m) => {
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

  const RF = mod.ReactFlow;
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
      // @xyflow/react v12 内部仍用 d3-drag, d3-drag 的 'start' event
      // 会在 mousedown 立即触发 startDrag 当且仅当 nodeDragThreshold === 0.
      // v12 默认不传这个 prop (undefined), 而 drag event 里的
      //   if (distance > nodeDragThreshold) startDrag()
      // 会因为 `distance > undefined` 永远 false, 节点拖动永远不响应.
      // 显式设 0 让 mousedown 立即进入 drag 状态.
      nodeDragThreshold={0}
      // 关键: 把 pane 平移限制到中键/右键 (0=左 1=中 2=右),
      // 否则左键拖动空地会被当成平移, 节点拖动会被干扰
      panOnDrag={[1, 2]}
      // 左键拖动空地改成框选
      selectionOnDrag
      // 拖动节点时同时选中
      selectNodesOnDrag
      fitView
      deleteKeyCode={["Delete", "Backspace"]}
    >
      <Background />
      <Controls />
      <MiniMap pannable zoomable />
    </RF>
  );
}
