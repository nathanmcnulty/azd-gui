const focusableSelector = [
  "a[href]",
  "button:not([disabled]):not([tabindex='-1'])",
  "input:not([disabled]):not([tabindex='-1'])",
  "select:not([disabled]):not([tabindex='-1'])",
  "textarea:not([disabled]):not([tabindex='-1'])",
  "[tabindex]:not([tabindex='-1'])"
].join(",");

export function trapFocus(node: HTMLDialogElement) {
  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;

    const focusable = Array.from(node.querySelectorAll<HTMLElement>(focusableSelector)).filter(
      (element) => !element.hidden && element.getAttribute("aria-hidden") !== "true"
    );
    const first = focusable[0];
    const last = focusable.at(-1);

    if (!first || !last) {
      event.preventDefault();
      node.focus();
      return;
    }

    const activeElement = document.activeElement;
    if (event.shiftKey && (activeElement === first || !node.contains(activeElement))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (activeElement === last || !node.contains(activeElement))) {
      event.preventDefault();
      first.focus();
    }
  }

  node.addEventListener("keydown", handleKeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", handleKeydown);
    }
  };
}
