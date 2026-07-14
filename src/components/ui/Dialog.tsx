import { useEffect } from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title?: string;
  description?: string;
  children?: React.ReactNode;
  footer?: React.ReactNode;
  className?: string;
  /** Tailwind max-w-* class, e.g. "max-w-2xl" */
  maxWidth?: string;
}

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  className,
  maxWidth,
}: DialogProps) {
  useEffect(() => {
    if (!open) return;
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") onOpenChange(false);
    };
    document.addEventListener("keydown", onEsc);
    return () => document.removeEventListener("keydown", onEsc);
  }, [open, onOpenChange]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={() => onOpenChange(false)}
    >
      <div
        className={cn(
          "max-h-[85vh] w-full max-w-lg overflow-hidden rounded-lg border bg-card text-card-foreground shadow-lg",
          maxWidth,
          className,
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {(title || description) && (
          <div className="flex items-start justify-between border-b px-4 py-3">
            <div>
              {title && <h2 className="text-base font-semibold">{title}</h2>}
              {description && (
                <p className="mt-0.5 text-sm text-muted-foreground">{description}</p>
              )}
            </div>
            <button
              className="text-muted-foreground hover:text-foreground"
              onClick={() => onOpenChange(false)}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        )}
        <div className="max-h-[60vh] overflow-auto p-4">{children}</div>
        {footer && (
          <div className="flex justify-end gap-2 border-t bg-muted/30 px-4 py-3">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
