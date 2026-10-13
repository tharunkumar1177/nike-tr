import { type KeyboardEvent, type ReactNode, useEffect, useId } from "react";
import type { ErcViolation } from "../bindings/ErcViolation";
import { CheckIcon, ErrorIcon, RemoveIcon, WarningIcon } from "./icons";

interface ErcPanelProps {
  /** `null` until the check has run. */
  violations: ErcViolation[] | null;
  /** The schematic changed since the last run. */
  stale: boolean;
  running: boolean;
  error: string | null;
  selectedId: string | null;
  /** Read by screen readers when a run finishes. */
  announcement: string;
  onRun: () => void;
  onSelect: (violation: ErcViolation) => void;
  /** Selects the violation and shows it on the canvas. */
  onJump: (violation: ErcViolation) => void;
  onClose: () => void;
}

export function ErcPanel({
  violations,
  stale,
  running,
  error,
  selectedId,
  announcement,
  onRun,
  onSelect,
  onJump,
  onClose,
}: ErcPanelProps) {
  const idPrefix = useId();
  const optionId = (index: number) => `${idPrefix}-violation-${index}`;
  const list = violations ?? [];
  const selectedIndex = list.findIndex((item) => item.id === selectedId);

  useEffect(() => {
    if (selectedIndex < 0) {
      return;
    }
    const option = document.getElementById(
      `${idPrefix}-violation-${selectedIndex}`,
    );
    if (option && typeof option.scrollIntoView === "function") {
      option.scrollIntoView({ block: "nearest" });
    }
  }, [idPrefix, selectedIndex]);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (list.length === 0) {
      return;
    }
    let next: number | null = null;
    if (event.key === "ArrowDown") {
      next = Math.min(list.length - 1, selectedIndex + 1);
    } else if (event.key === "ArrowUp") {
      next = Math.max(0, selectedIndex - 1);
    } else if (event.key === "Home") {
      next = 0;
    } else if (event.key === "End") {
      next = list.length - 1;
    } else if (event.key === "Enter") {
      const current = list[selectedIndex];
      if (current) {
        event.preventDefault();
        onJump(current);
      }
      return;
    }
    const violation = next === null ? undefined : list[next];
    if (violation) {
      event.preventDefault();
      onSelect(violation);
    }
  };

  const groups = [
    { severity: "error", title: "Errors" },
    { severity: "warning", title: "Warnings" },
  ] as const;

  let content: ReactNode;
  if (running) {
    content = (
      <p className="erc-panel__state" role="status">
        Running checks...
      </p>
    );
  } else if (error !== null) {
    content = (
      <p className="form__error erc-panel__state" role="alert">
        {error}
      </p>
    );
  } else if (violations === null) {
    content = (
      <div className="erc-panel__state">
        <p className="erc-panel__note">
          Run ERC to check for electrical rules violations.
        </p>
        <button
          type="button"
          className="button button--primary"
          onClick={onRun}
        >
          Run ERC
        </button>
      </div>
    );
  } else if (violations.length === 0) {
    content = (
      <p className="erc-panel__state erc-panel__clean" role="status">
        <CheckIcon />
        No violations found.
      </p>
    );
  } else {
    content = (
      <div
        className="erc-panel__list"
        role="listbox"
        aria-label="Violations"
        aria-activedescendant={
          selectedIndex >= 0 ? optionId(selectedIndex) : undefined
        }
        tabIndex={0}
        onKeyDown={handleKeyDown}
      >
        {groups.map(({ severity, title }) => {
          const members = list.filter((item) => item.severity === severity);
          if (members.length === 0) {
            return null;
          }
          const headerId = `${idPrefix}-${severity}`;
          return (
            <div key={severity} role="group" aria-labelledby={headerId}>
              <div id={headerId} className="erc-panel__group">
                {title} ({members.length})
              </div>
              {members.map((violation) => {
                const index = list.indexOf(violation);
                return (
                  <div
                    key={violation.id}
                    id={optionId(index)}
                    role="option"
                    aria-selected={index === selectedIndex}
                    className="erc-row"
                    onClick={() => onJump(violation)}
                  >
                    <span
                      className={`erc-row__icon erc-row__icon--${severity}`}
                    >
                      {severity === "error" ? <ErrorIcon /> : <WarningIcon />}
                    </span>
                    <span className="erc-row__text">
                      <span className="erc-row__message">
                        {violation.message}
                      </span>
                      <span className="erc-row__detail">
                        {violation.items.join(", ")}
                      </span>
                    </span>
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    );
  }

  const showRun = violations !== null || error !== null;
  return (
    <section className="erc-panel" aria-label="Electrical Rules Checker">
      <header className="erc-panel__header">
        <h2 className="erc-panel__title">Electrical Rules Checker</h2>
        {stale && <span className="erc-panel__badge">Out of date</span>}
        <div className="erc-panel__actions">
          {showRun && (
            <button
              type="button"
              className={stale ? "button button--primary" : "button"}
              disabled={running}
              onClick={onRun}
            >
              Run ERC
            </button>
          )}
          <button
            type="button"
            className="icon-button"
            aria-label="Close Electrical Rules Checker"
            onClick={onClose}
          >
            <RemoveIcon />
          </button>
        </div>
      </header>
      <div className="erc-panel__content">{content}</div>
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
    </section>
  );
}
