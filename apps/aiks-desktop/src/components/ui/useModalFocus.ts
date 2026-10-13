import { useEffect, useRef } from "react";

// The last mounted open dialog owns Escape and focus containment.
const modalStack: Array<object> = [];

/** Keep keyboard focus within an open desktop overlay and restore its trigger on close. */
export function useModalFocus<T extends HTMLElement = HTMLDivElement>(open: boolean, onClose: () => void, closeEnabled = true) {
  const containerRef = useRef<T>(null);
  const closeRef = useRef(onClose);
  const enabledRef = useRef(closeEnabled);
  closeRef.current = onClose;
  enabledRef.current = closeEnabled;

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const token = {};
    modalStack.push(token);
    const handleKeyDown = (event: KeyboardEvent) => {
      if (modalStack[modalStack.length - 1] !== token || event.defaultPrevented) return;
      if (event.key === "Escape") {
        // Popovers get the first Escape: do not close their enclosing dialog.
        const source = event.target;
        if (source instanceof Element && source.closest('[data-aiks-popup="true"]')) return;
        if (!enabledRef.current) return;
        event.preventDefault();
        event.stopPropagation();
        closeRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      if (event.altKey || event.ctrlKey || event.metaKey) return;
      const container = containerRef.current;
      if (!container) return;
      const items = Array.from(container.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])'
      )).filter(item => item.getClientRects().length > 0 && !item.closest('[aria-hidden="true"], [inert]'));
      if (items.length === 0) {
        event.preventDefault();
        container.focus();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (event.shiftKey && (active === first || !container.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || !container.contains(active))) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
      const index = modalStack.lastIndexOf(token);
      const wasTopmost = index === modalStack.length - 1;
      if (index >= 0) modalStack.splice(index, 1);
      if (wasTopmost && previous?.isConnected) previous.focus();
    };
  }, [open]);
  return containerRef;
}
