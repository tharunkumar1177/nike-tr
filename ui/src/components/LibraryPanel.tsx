import {
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import type { LibraryFootprint } from "../bindings/LibraryFootprint";
import type { LibraryHit } from "../bindings/LibraryHit";
import type { LibrarySymbol } from "../bindings/LibrarySymbol";
import type { ReadyLibrary } from "../bindings/ReadyLibrary";
import {
  type ErrorReport,
  MAX_LIBRARY_RESULTS,
  getLibraryFootprint,
  getLibrarySymbol,
  searchLibrary,
  toErrorReport,
} from "../ipc";
import type { LibraryStatusState } from "../useLibraryStatus";
import { LibraryPreview, type PreviewGeometry } from "./LibraryPreview";

/** How long typing must pause before a search is sent. */
export const SEARCH_DELAY_MS = 150;

type Results =
  | { status: "idle" }
  | { status: "searching"; hits: LibraryHit[] }
  | { status: "done"; query: string; hits: LibraryHit[] }
  | { status: "error"; error: ErrorReport };

type Detail =
  | { status: "loading" }
  | { status: "symbol"; symbol: LibrarySymbol }
  | { status: "footprint"; footprint: LibraryFootprint }
  | { status: "unavailable" };

function kindLabel(hit: LibraryHit): string {
  return hit.kind === "symbol" ? "Symbol" : "Footprint";
}

function useDetail(hit: LibraryHit | null): Detail | null {
  const [detail, setDetail] = useState<Detail | null>(null);

  useEffect(() => {
    if (hit === null) {
      setDetail(null);
      return;
    }
    let current = true;
    setDetail({ status: "loading" });
    const loaded: Promise<Detail> =
      hit.kind === "symbol"
        ? getLibrarySymbol(hit.id).then((symbol) =>
            symbol ? { status: "symbol", symbol } : { status: "unavailable" },
          )
        : getLibraryFootprint(hit.id).then((footprint) =>
            footprint
              ? { status: "footprint", footprint }
              : { status: "unavailable" },
          );
    loaded.then(
      (next) => {
        if (current) {
          setDetail(next);
        }
      },
      () => {
        if (current) {
          setDetail({ status: "unavailable" });
        }
      },
    );
    return () => {
      current = false;
    };
  }, [hit]);

  return detail;
}

function PreviewArea({ hit }: { hit: LibraryHit }) {
  const detail = useDetail(hit);
  const geometry = useMemo<PreviewGeometry | null>(() => {
    if (detail?.status === "symbol") {
      return { kind: "symbol", preview: detail.symbol.preview };
    }
    if (detail?.status === "footprint") {
      return { kind: "footprint", preview: detail.footprint.preview };
    }
    return null;
  }, [detail]);

  return (
    <section className="library-preview" aria-label="Preview">
      {geometry ? (
        <LibraryPreview
          label={`${kindLabel(hit)} ${hit.lib}:${hit.name}`}
          geometry={geometry}
        />
      ) : (
        <div className="library-preview__graphic">
          <p className="library-preview__notice">
            {detail?.status === "unavailable"
              ? "Preview unavailable"
              : "Loading preview..."}
          </p>
        </div>
      )}
      <p className="library-preview__name">
        {hit.lib}:{hit.name}
      </p>
      {hit.description && (
        <p className="library-preview__desc">{hit.description}</p>
      )}
    </section>
  );
}

interface LibraryBrowserProps {
  library: ReadyLibrary;
  onPlaceSymbol: (hit: LibraryHit) => void;
  onSymbolDragStart: (hit: LibraryHit) => void;
  onSymbolDragEnd: () => void;
}

function LibraryBrowser({
  library,
  onPlaceSymbol,
  onSymbolDragStart,
  onSymbolDragEnd,
}: LibraryBrowserProps) {
  const listId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const optionRefs = useRef<(HTMLLIElement | null)[]>([]);
  const requestRef = useRef(0);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Results>({ status: "idle" });
  const [selected, setSelected] = useState<number | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const trimmed = query.trim();
    const request = ++requestRef.current;
    if (trimmed === "") {
      setResults({ status: "idle" });
      setSelected(null);
      return;
    }
    setResults((previous) => ({
      status: "searching",
      hits: "hits" in previous ? previous.hits : [],
    }));
    const timer = window.setTimeout(() => {
      searchLibrary(trimmed).then(
        (hits) => {
          if (request === requestRef.current) {
            setResults({ status: "done", query: trimmed, hits });
            setSelected(null);
          }
        },
        (error: unknown) => {
          if (request === requestRef.current) {
            setResults({ status: "error", error: toErrorReport(error) });
            setSelected(null);
          }
        },
      );
    }, SEARCH_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [query]);

  const hits = "hits" in results ? results.hits : [];
  const selectedHit = selected === null ? null : (hits[selected] ?? null);

  function select(index: number) {
    if (index < 0 || index >= hits.length) {
      return;
    }
    setSelected(index);
    optionRefs.current[index]?.focus();
  }

  function onInputKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" && hits.length > 0) {
      event.preventDefault();
      select(0);
    }
  }

  function onListKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    if (event.key === "Enter") {
      if (selectedHit?.kind === "symbol") {
        event.preventDefault();
        onPlaceSymbol(selectedHit);
      }
      return;
    }
    const current = selected ?? -1;
    const moves: Record<string, number> = {
      ArrowDown: current + 1,
      ArrowUp: current - 1,
      Home: 0,
      End: hits.length - 1,
    };
    const next = moves[event.key];
    if (next === undefined) {
      return;
    }
    event.preventDefault();
    if (event.key === "ArrowUp" && current === 0) {
      inputRef.current?.focus();
      return;
    }
    select(next);
  }

  return (
    <>
      <div className="library-panel__search">
        <input
          ref={inputRef}
          className="input"
          type="search"
          aria-label="Search symbols and footprints"
          aria-controls={listId}
          placeholder="Search symbols and footprints..."
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onInputKeyDown}
        />
      </div>
      <div className="library-panel__results">
        {results.status === "searching" && (
          <p className="welcome__empty library-panel__note" role="status">
            Searching...
          </p>
        )}
        {results.status === "error" && (
          <p className="form__error library-panel__note" role="alert">
            {results.error.message}
          </p>
        )}
        {results.status === "done" && hits.length === 0 && (
          <p className="welcome__empty library-panel__note" role="status">
            No matches found for &quot;{results.query}&quot;.
          </p>
        )}
        {hits.length > 0 && (
          <ul
            id={listId}
            className="recent-list"
            role="listbox"
            aria-label="Search results"
            onKeyDown={onListKeyDown}
          >
            {hits.map((hit, index) => (
              <li
                key={`${hit.kind}:${hit.id}`}
                ref={(element) => {
                  optionRefs.current[index] = element;
                }}
                className="recent-list__item library-list__item"
                role="option"
                aria-selected={index === selected}
                tabIndex={
                  index === selected || (selected === null && index === 0)
                    ? 0
                    : -1
                }
                onClick={() => select(index)}
                onDoubleClick={() => {
                  if (hit.kind === "symbol") {
                    onPlaceSymbol(hit);
                  }
                }}
                draggable={hit.kind === "symbol"}
                onDragStart={(event) => {
                  if (hit.kind !== "symbol") {
                    return;
                  }
                  event.dataTransfer.setData(
                    "text/plain",
                    `${hit.lib}:${hit.name}`,
                  );
                  event.dataTransfer.effectAllowed = "copy";
                  onSymbolDragStart(hit);
                }}
                onDragEnd={() => {
                  if (hit.kind === "symbol") {
                    onSymbolDragEnd();
                  }
                }}
              >
                <span className="recent-list__name">
                  {hit.name}
                  <span className="library-item__kind">{kindLabel(hit)}</span>
                </span>
                <span className="recent-list__path">{hit.lib}</span>
              </li>
            ))}
          </ul>
        )}
        {results.status === "done" && hits.length >= MAX_LIBRARY_RESULTS && (
          <p className="welcome__empty library-panel__note">
            Top {MAX_LIBRARY_RESULTS} results shown. Refine your search.
          </p>
        )}
      </div>
      {selectedHit && <PreviewArea hit={selectedHit} />}
      <p className="welcome__version library-panel__attribution">
        {library.attribution}
      </p>
    </>
  );
}

interface LibraryPanelProps {
  library: LibraryStatusState;
  onDownload: () => void;
  onPlaceSymbol: (hit: LibraryHit) => void;
  onSymbolDragStart: (hit: LibraryHit) => void;
  onSymbolDragEnd: () => void;
}

export function LibraryPanel({
  library,
  onDownload,
  onPlaceSymbol,
  onSymbolDragStart,
  onSymbolDragEnd,
}: LibraryPanelProps) {
  let content: ReactNode;
  if (library.status === "loading") {
    content = <p className="welcome__empty library-panel__note">Loading...</p>;
  } else if (library.status === "error") {
    content = (
      <p className="form__error library-panel__note" role="alert">
        {library.error.message}
      </p>
    );
  } else if (library.library.state === "ready") {
    content = (
      <LibraryBrowser
        library={library.library}
        onPlaceSymbol={onPlaceSymbol}
        onSymbolDragStart={onSymbolDragStart}
        onSymbolDragEnd={onSymbolDragEnd}
      />
    );
  } else {
    content = (
      <div className="app-status__content library-panel__empty">
        <h2 className="app-status__title">Library not installed</h2>
        <p className="app-status__secondary">
          Download the component library to search symbols and footprints.
        </p>
        <button type="button" className="button" onClick={onDownload}>
          Download Library
        </button>
      </div>
    );
  }
  return (
    <aside className="library-sidebar" aria-label="Component library">
      {content}
    </aside>
  );
}
