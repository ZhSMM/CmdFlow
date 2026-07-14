import { useEffect, useImperativeHandle, useRef, forwardRef } from "react";
import type { Terminal as XTermType } from "@xterm/xterm";
import type { FitAddon as FitAddonType } from "@xterm/addon-fit";
import { cn } from "@/lib/utils";

type XTerm = XTermType;
type FitAddon = FitAddonType;

export interface TerminalHandle {
  write: (data: string) => void;
  writeln: (data: string) => void;
  clear: () => void;
  fit: () => void;
}

interface TerminalProps {
  className?: string;
  onReady?: (term: TerminalHandle) => void;
}

type BufferedOp =
  | { kind: "write"; content: string }
  | { kind: "writeln"; content: string };

/**
 * xterm.js 封装，dark 主题，支持 ANSI 颜色
 */
export const Terminal = forwardRef<TerminalHandle, TerminalProps>(
  ({ className, onReady }, ref) => {
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef = useRef<XTerm | null>(null);
    const fitRef = useRef<FitAddon | null>(null);
    // xterm 是动态 import 的，加载完成前调用 write/writeln 会被吞。
    // 这里把调用暂存到 buffer，XTerm 起来后一次性回放。
    const bufferRef = useRef<BufferedOp[]>([]);

    useEffect(() => {
      if (!containerRef.current) return;

      // StrictMode 下 effect 会被双调用：第一次的 cleanup 会在 async 完成前跑，
      // `dispose` 还来不及赋值，导致第一个 xterm 永远不被释放，叠在容器上。
      // 用 cancelled 标记：cleanup 立即置 true，async 完成时如果 cancelled 就
      // 直接放弃创建，杜绝双实例。
      let cancelled = false;
      let term: XTerm | null = null;
      let fit: FitAddon | null = null;
      let ro: ResizeObserver | null = null;
      const onResize = () => {
        try {
          fit?.fit();
        } catch {
          // ignore
        }
      };

      // 动态 import xterm (~310KB) - 不进主 bundle
      (async () => {
        const [{ Terminal: XTerm }, { FitAddon }, cssMod] = await Promise.all([
          import("@xterm/xterm"),
          import("@xterm/addon-fit"),
          import("@xterm/xterm/css/xterm.css"),
        ]);
        void cssMod;

        // effect 已被清理（StrictMode 第二次 mount 之前），不再创建 xterm
        if (cancelled || !containerRef.current) return;

        const newTerm = new XTerm({
          theme: {
            background: "#0f172a",
            foreground: "#e2e8f0",
            cursor: "#4ade80",
            selectionBackground: "#475569",
          },
          fontFamily:
            '"Cascadia Code", "JetBrains Mono", "Fira Code", Consolas, monospace',
          fontSize: 12,
          lineHeight: 1.3,
          cursorBlink: false,
          convertEol: true,
          disableStdin: true,
          scrollback: 5000,
        });
        const newFit = new FitAddon();
        newTerm.loadAddon(newFit);
        newTerm.open(containerRef.current!);

        try {
          newFit.fit();
        } catch {
          // ignore
        }

        // 先把缓冲回放，再开放实时写入
        const buffered = bufferRef.current;
        for (const op of buffered) {
          if (op.kind === "write") {
            newTerm.write(op.content);
          } else {
            newTerm.writeln(op.content);
          }
        }
        bufferRef.current = [];

        term = newTerm;
        fit = newFit;
        termRef.current = newTerm;
        fitRef.current = newFit;

        ro = new ResizeObserver(onResize);
        ro.observe(containerRef.current!);
        window.addEventListener("resize", onResize);

        onReady?.({
          write: (s) => newTerm.write(s),
          writeln: (s) => newTerm.writeln(s),
          clear: () => newTerm.clear(),
          fit: () => newFit.fit(),
        });
      })().catch((e) => {
        console.error("Failed to load xterm:", e);
      });

      return () => {
        cancelled = true;
        if (ro) {
          ro.disconnect();
          ro = null;
        }
        window.removeEventListener("resize", onResize);
        if (term) {
          term.dispose();
        }
        if (termRef.current === term) {
          termRef.current = null;
        }
        if (fitRef.current === fit) {
          fitRef.current = null;
        }
        term = null;
        fit = null;
      };
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    useImperativeHandle(ref, () => ({
      write: (s) => {
        const term = termRef.current;
        if (term) {
          term.write(s);
        } else {
          bufferRef.current.push({ kind: "write", content: s });
        }
      },
      writeln: (s) => {
        const term = termRef.current;
        if (term) {
          term.writeln(s);
        } else {
          bufferRef.current.push({ kind: "writeln", content: s });
        }
      },
      clear: () => {
        if (termRef.current) {
          termRef.current.clear();
        } else {
          bufferRef.current = [];
        }
      },
      fit: () => {
        fitRef.current?.fit();
      },
    }));

    return (
      <div
        ref={containerRef}
        className={cn("h-full w-full overflow-hidden rounded-md bg-[#0f172a] p-2", className)}
      />
    );
  },
);
Terminal.displayName = "Terminal";
