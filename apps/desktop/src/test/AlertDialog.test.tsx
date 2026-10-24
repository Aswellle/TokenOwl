import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { useRef } from "react";

import { AlertDialog } from "../components/AlertDialog";

function Harness() {
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <div>
      <button ref={trigger} type="button">
        trigger
      </button>
      <AlertDialog
        title="确认操作"
        body="这个操作会影响账户。"
        confirmLabel="我确认，继续"
        cancelLabel="取消"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        returnFocusTo={trigger}
        className="lan-consent"
      />
    </div>
  );
}

function buttons() {
  const cancel = screen.getByRole("button", { name: "取消" });
  const confirm = screen.getByRole("button", { name: "我确认，继续" });
  return { cancel, confirm, dialog: cancel.closest('[role="alertdialog"]')! };
}

describe("AlertDialog accessibility contract", () => {
  beforeEach(() => {
    cleanup();
  });

  it("announces itself with a label, description, and modal semantics", () => {
    render(<Harness />);
    const { dialog } = buttons();
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const labelledBy = dialog.getAttribute("aria-labelledby");
    const describedBy = dialog.getAttribute("aria-describedby");
    expect(labelledBy).toBeTruthy();
    expect(describedBy).toBeTruthy();
    expect(document.getElementById(labelledBy!)).toHaveTextContent("确认操作");
    expect(document.getElementById(describedBy!)).toHaveTextContent("这个操作会影响账户。");
  });

  it("puts the initial focus on the non-destructive action", () => {
    render(<Harness />);
    const { cancel } = buttons();
    expect(document.activeElement).toBe(cancel);
  });

  it("keeps Tab cycling inside the dialog in both directions", () => {
    render(<Harness />);
    const { cancel, confirm, dialog } = buttons();
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(confirm);
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(cancel);
    fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(confirm);
  });

  it("Escape cancels and returns focus to the trigger", () => {
    render(<Harness />);
    const { dialog } = buttons();
    const trigger = screen.getByRole("button", { name: "trigger" });
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(document.activeElement).toBe(trigger);
  });

  it("confirming also returns focus to the trigger", () => {
    render(<Harness />);
    const { confirm } = buttons();
    const trigger = screen.getByRole("button", { name: "trigger" });
    fireEvent.click(confirm);
    expect(document.activeElement).toBe(trigger);
  });

  it("routes the two buttons to their handlers exactly once per click", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <AlertDialog
        title="t"
        body="b"
        confirmLabel="confirm"
        cancelLabel="cancel"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "confirm" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
