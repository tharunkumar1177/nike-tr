import { useCallback, useEffect, useState } from "react";
import type { AppInfo } from "./bindings/AppInfo";
import { describeIpcError, getAppInfo } from "./ipc";

type AppInfoState =
  | { status: "loading" }
  | { status: "loaded"; info: AppInfo }
  | { status: "error"; message: string };

export function App() {
  const [state, setState] = useState<AppInfoState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    getAppInfo().then(
      (info) => {
        if (active) {
          setState({ status: "loaded", info });
        }
      },
      (error: unknown) => {
        if (active) {
          setState({ status: "error", message: describeIpcError(error) });
        }
      },
    );
    return () => {
      active = false;
    };
  }, [attempt]);

  const retry = useCallback(() => {
    setState({ status: "loading" });
    setAttempt((previous) => previous + 1);
  }, []);

  return (
    <main className="app-status">
      <div className="app-status__content" aria-live="polite">
        {state.status === "loading" && (
          <p className="app-status__secondary">Connecting to core...</p>
        )}
        {state.status === "loaded" && (
          <>
            <h1 className="app-status__title">{state.info.name}</h1>
            <p className="app-status__secondary">{state.info.version}</p>
          </>
        )}
        {state.status === "error" && (
          <>
            <h1 className="app-status__title app-status__title--error">
              Failed to load app info.
            </h1>
            <p className="app-status__secondary">{state.message}</p>
            <button type="button" className="button" onClick={retry}>
              Retry
            </button>
          </>
        )}
      </div>
    </main>
  );
}
