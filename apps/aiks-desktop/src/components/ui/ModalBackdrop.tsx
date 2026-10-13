import * as Dialog from "@radix-ui/react-dialog";
import type { ReactNode } from "react";

type OverlayKind = "editor" | "search" | "assistant";
const layouts: Record<OverlayKind, string> = {
  editor: "fixed inset-0 z-[80] flex items-center justify-center p-2 sm:p-6",
  search: "fixed inset-0 z-[80] flex items-start justify-center px-2 pt-4 sm:px-6 sm:pt-16",
  assistant: "fixed inset-0 z-[90]",
};
const overlays: Record<OverlayKind, string> = {
  editor: "bg-black/40 backdrop-blur-sm",
  search: "bg-black/20 backdrop-blur-[1px]",
  assistant: "bg-black/15 backdrop-blur-[1px]",
};

/** Radix owns focus trapping, Escape, outside dismissal, and trigger focus return. */
export function ModalBackdrop({ kind, onClose, children, className = "", dismissOnBackdrop = true }: {
  kind: OverlayKind; onClose: () => void; children: ReactNode; className?: string; dismissOnBackdrop?: boolean;
}) {
  return (
    <Dialog.Root open onOpenChange={next => { if (!next && dismissOnBackdrop) onClose(); }}>
      <Dialog.Portal>
        <Dialog.Overlay className={`fixed inset-0 z-[79] ${overlays[kind]}`} />
        <div className={`${layouts[kind]} ${className} pointer-events-none`}>
          <Dialog.Content asChild
            onPointerDownOutside={event => { if (!dismissOnBackdrop) event.preventDefault(); }}
            onEscapeKeyDown={event => { if (!dismissOnBackdrop) event.preventDefault(); }}>
            {children}
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
