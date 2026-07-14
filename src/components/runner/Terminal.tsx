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

/**
 * xterm.js 封装，dark 主题，支持 ANSI 颜色
 */
export const Terminal = forwardRef<TerminalHandle, TerminalProps>(
  ({ className, onReady }, ref) => {
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef = useRef<XTerm | null>(null);
    const fitRef = useRef<FitAddon | null>(null);

    useEffect(() => {
      if (!containerRef.current) return;

      let dispose: (() => void) | null = null;

      // 动态 import xterm (~310KB) - 不进主 bundle
      (async () => {
        const [{ Terminal: XTerm }, { FitAddon }, cssMod] = await Promise.all([
          import("@xterm/xterm"),
          import("@xterm/addon-fit"),
          import("@xterm/xterm/css/xterm.css"),
        ]);
        void cssMod;

        const term = new XTerm({
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
        const fit = new FitAddon();
        term.loadAddon(fit);
        term.open(containerRef.current!);

        try {
          fit.fit();
        } catch {
          // ignore
        }

        termRef.current = term;
        fitRef.current = fit;

        const resize = () => {
          try {
            fit.fit();
          } catch {
            // ignore
          }
        };
        const ro = new ResizeObserver(resize);
        ro.observe(containerRef.current!);
        window.addEventListener("resize", resize);

        onReady?.({
          write: (s) => term.write(s),
          writeln: (s) => term.writeln(s),
          clear: () => term.clear(),
          fit: () => fit.fit(),
        });

        dispose = () => {
          ro.disconnect();
          window.removeEventListener("resize", resize);
          term.dispose();
          termRef.current = null;
          fitRef.current = null;
        };
      })();

      return () => {
        if (dispose) dispose();
      };
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    useImperativeHandle(ref, () => ({
      write: (s) => termRef.current?.write(s),
      writeln: (s) => termRef.current?.writeln(s),
      clear: () => termRef.current?.clear(),
      fit: () => fitRef.current?.fit(),
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
