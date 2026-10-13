/**
 * The power symbol picker that opens at the cursor: the common supply and
 * ground symbols from the component library, plus a way into the full
 * library search.
 */
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { LibraryHit } from "../bindings/LibraryHit";
import { describeIpcError } from "../ipc";
import { findPowerSymbols } from "../powerSymbols";
import type { LibraryStatusState } from "../useLibraryStatus";
import type { ScreenPoint } from "../viewport";

type Options =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; hits: LibraryHit[] };

interface PowerPickerProps {
  /** Where the picker opens, in pixels from the canvas's top left. */
  at: ScreenPoint;
  library: LibraryStatusState;
  onPick: (hit: LibraryHit) => void;
  onSearchLibrary: () => void;
  onDownloadLibrary: () => void;
  onCancel: () => void;
}

export function PowerPicker({
  at,
  library,
  onPick,
  onSearchLibrary,
  onDownloadLibrary,
  onCancel,
}: PowerPickerProps) {
  const listRef = useRef<HTMLUListElement>(null);
  const [options, setOptions] = useState<Options>({ status: "loading" });
  const [active, setActive] = useState(0);
  const ready = library.status === "ready" && library.library.state === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    let disposed = false;
    findPowerSymbols().then(
      (hits) => {
        if (!disposed) {
          setOptions({ status: "ready", hits });
        }
      },
      (error: unknown) => {
        if (!disposed) {
          setOptions({ status: "error", message: describeIpcError(error) });
        }
      },
    );
    return () => {
      disposed = true;
    };
  }, [ready]);

  const hits = options.status === "ready" ? options.hits : [];
  // The last option is always "Search library...".
  const count = hits.length + 1;

  useEffect(() => {
    listRef.current?.focus();
  }, [ready]);

  const choose = (index: number) => {
    const hit = hits[index];
    if (hit) {
      onPick(hit);
    } else {
      onSearchLibrary();
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((index) => (index + 1) % count);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((index) => (index - 1 + count) % count);
    } else if (event.key === "Enter" && ready) {
      event.preventDefault();
      choose(active);
    }
  };

  if (!ready) {
    return (
      <div
        className="canvas-popover"
        style={{ left: at.x, top: at.y }}
        role="dialog"
        aria-label="Power symbols"
        onKeyDown={handleKeyDown}
      >
        <p className="field__hint">Component library required</p>
        <div className="canvas-popover__actions">
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="button button--primary"
            autoFocus
            onClick={onDownloadLibrary}
          >
            Download
          </button>
        </div>
      </div>
    );
  }

  const optionId = (index: number) => `power-option-${index}`;
  return (
    <div
      className="canvas-popover canvas-popover--menu"
      style={{ left: at.x, top: at.y }}
    >
      <ul
        ref={listRef}
        className="canvas-popover__list"
        role="listbox"
        aria-label="Power symbols"
        aria-activedescendant={optionId(active)}
        aria-busy={options.status === "loading"}
        tabIndex={0}
        onKeyDown={handleKeyDown}
      >
        {hits.map((hit, index) => (
          <li
            key={hit.id}
            id={optionId(index)}
            role="option"
            aria-selected={index === active}
            className="canvas-popover__option"
            onPointerEnter={() => setActive(index)}
            onClick={() => choose(index)}
          >
            {hit.name}
          </li>
        ))}
        <li
          id={optionId(hits.length)}
          role="option"
          aria-selected={active === hits.length}
          className="canvas-popover__option canvas-popover__option--more"
          onPointerEnter={() => setActive(hits.length)}
          onClick={() => choose(hits.length)}
        >
          Search library...
        </li>
      </ul>
      {options.status === "loading" && (
        <p className="field__hint">Loading power symbols...</p>
      )}
      {options.status === "error" && (
        <p className="form__error" role="alert">
          {options.message}
        </p>
      )}
    </div>
  );
}
