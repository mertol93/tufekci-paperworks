import { useEffect, useRef, type RefObject } from "react";

const dialogFocusableSelector = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "details > summary:first-of-type",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])"
].join(",");

export type RovingNavigationKey =
  | "ArrowDown"
  | "ArrowLeft"
  | "ArrowRight"
  | "ArrowUp"
  | "End"
  | "Home";

export function rovingNavigationIndex(
  currentIndex: number,
  itemCount: number,
  key: string
): number | null {
  if (itemCount <= 0 || currentIndex < 0 || currentIndex >= itemCount) {
    return null;
  }

  if (key === "Home") {
    return 0;
  }
  if (key === "End") {
    return itemCount - 1;
  }
  if (key === "ArrowDown" || key === "ArrowRight") {
    return (currentIndex + 1) % itemCount;
  }
  if (key === "ArrowUp" || key === "ArrowLeft") {
    return (currentIndex - 1 + itemCount) % itemCount;
  }
  return null;
}

type DialogFocusOptions = {
  active: boolean;
  escapeDisabled?: boolean;
  onEscape?: () => void;
};

function visibleFocusableElements(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(dialogFocusableSelector)).filter(
    (element) =>
      !element.matches(":disabled") &&
      element.getAttribute("aria-hidden") !== "true" &&
      element.getClientRects().length > 0
  );
}

function focusDialogStart(root: HTMLElement): void {
  const focusable = visibleFocusableElements(root);
  const preferred = focusable.find((element) => element.hasAttribute("data-dialog-initial-focus"));
  (preferred ?? focusable[0] ?? root).focus({ preventScroll: true });
}

export function useDialogFocus<T extends HTMLElement>({
  active,
  escapeDisabled = false,
  onEscape
}: DialogFocusOptions): RefObject<T> {
  const dialogRef = useRef<T>(null);
  const escapeDisabledRef = useRef(escapeDisabled);
  const onEscapeRef = useRef(onEscape);

  escapeDisabledRef.current = escapeDisabled;
  onEscapeRef.current = onEscape;

  useEffect(() => {
    if (!active) {
      return;
    }

    const previouslyFocused =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusFrame = window.requestAnimationFrame(() => {
      if (dialogRef.current) {
        focusDialogStart(dialogRef.current);
      }
    });

    const handleKeyDown = (event: KeyboardEvent) => {
      const root = dialogRef.current;
      if (!root) {
        return;
      }

      if (event.key === "Escape" && onEscapeRef.current && !escapeDisabledRef.current) {
        event.preventDefault();
        event.stopPropagation();
        onEscapeRef.current();
        return;
      }

      if (event.key !== "Tab") {
        return;
      }

      const focusable = visibleFocusableElements(root);
      if (focusable.length === 0) {
        event.preventDefault();
        root.focus({ preventScroll: true });
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const focused = document.activeElement;
      if (!root.contains(focused)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (event.shiftKey && focused === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && focused === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.cancelAnimationFrame(focusFrame);
      document.removeEventListener("keydown", handleKeyDown, true);
      if (previouslyFocused?.isConnected) {
        window.requestAnimationFrame(() => previouslyFocused.focus({ preventScroll: true }));
      }
    };
  }, [active]);

  return dialogRef;
}
