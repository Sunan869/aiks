import type { MouseEvent, ReactNode } from "react";
type OverlayKind = "editor" | "search" | "assistant";
const layouts: Record<OverlayKind, string> = {
  editor: "fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-2 backdrop-blur-sm sm:p-6",
  search: "absolute inset-0 z-50 flex items-start justify-center bg-black/20 px-2 pt-4 backdrop-blur-[1px] sm:px-6 sm:pt-16",
  assistant: "fixed inset-0 z-[80] bg-black/15 backdrop-blur-[1px]",
};
/** Outside clicks are handled at the overlay boundary, never by its dialog content. */
export function ModalBackdrop({ kind, onClose, children, className = "", dismissOnBackdrop = true }: {
  kind: OverlayKind; onClose: () => void; children: ReactNode; className?: string; dismissOnBackdrop?: boolean;
}) {
  const handleMouseDown = (event: MouseEvent<HTMLDivElement>) => {
    if (dismissOnBackdrop && event.target === event.currentTarget) onClose();
  };
  return <div className={`${layouts[kind]} ${className}`} onMouseDown={handleMouseDown}>{children}</div>;
}
