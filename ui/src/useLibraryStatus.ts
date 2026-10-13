import { useEffect, useState } from "react";
import type { LibraryStatus } from "./bindings/LibraryStatus";
import {
  type ErrorReport,
  LIBRARY_STATUS_EVENT,
  getLibraryStatus,
  subscribe,
  toErrorReport,
} from "./ipc";

export type LibraryStatusState =
  | { status: "loading" }
  | { status: "error"; error: ErrorReport }
  | { status: "ready"; library: LibraryStatus };

/**
 * The component library status, kept current by `library-status` events.
 * Every event carries the whole status, so once one has arrived, the reply
 * to the initial request is older and is dropped.
 */
export function useLibraryStatus(): [
  LibraryStatusState,
  (library: LibraryStatus) => void,
] {
  const [state, setState] = useState<LibraryStatusState>({
    status: "loading",
  });

  useEffect(() => {
    let eventSeen = false;
    let disposed = false;
    const unsubscribe = subscribe<LibraryStatus>(
      LIBRARY_STATUS_EVENT,
      (library) => {
        eventSeen = true;
        setState({ status: "ready", library });
      },
    );
    getLibraryStatus().then(
      (library) => {
        if (!disposed && !eventSeen) {
          setState({ status: "ready", library });
        }
      },
      (error: unknown) => {
        if (!disposed && !eventSeen) {
          setState({ status: "error", error: toErrorReport(error) });
        }
      },
    );
    return () => {
      disposed = true;
      unsubscribe();
    };
  }, []);

  const adopt = (library: LibraryStatus) =>
    setState({ status: "ready", library });
  return [state, adopt];
}

/** Formats a byte count in megabytes with one decimal, such as `16.5`. */
export function formatMegabytes(bytes: number): string {
  return (bytes / 1_000_000).toFixed(1);
}
