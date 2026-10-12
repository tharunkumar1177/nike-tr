import { useCallback, useEffect, useRef, useState } from "react";
import type { DocumentDiff } from "./bindings/DocumentDiff";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import { applyDiff } from "./documentDiff";
import {
  DOCUMENT_CHANGED_EVENT,
  type ErrorReport,
  getProject,
  subscribe,
  toErrorReport,
} from "./ipc";

/**
 * Mirrors the open project from the core. `project` is `undefined` until the
 * first snapshot arrives and `null` when no project is open.
 */
export function useProjectDocument() {
  const [project, setProjectState] = useState<
    ProjectSnapshot | null | undefined
  >(undefined);
  const [loadError, setLoadError] = useState<ErrorReport | null>(null);
  const projectRef = useRef<ProjectSnapshot | null | undefined>(undefined);

  const setProject = useCallback((next: ProjectSnapshot | null) => {
    projectRef.current = next;
    setProjectState(next);
  }, []);

  const resync = useCallback(async () => {
    try {
      const snapshot = await getProject();
      setLoadError(null);
      setProject(snapshot);
    } catch (error) {
      setLoadError(toErrorReport(error));
    }
  }, [setProject]);

  useEffect(() => {
    void resync();
  }, [resync]);

  useEffect(() => {
    return subscribe<DocumentDiff>(DOCUMENT_CHANGED_EVENT, (diff) => {
      const current = projectRef.current;
      if (!current) {
        return;
      }
      const next = applyDiff(current, diff);
      if (next) {
        setProject(next);
      } else {
        void resync();
      }
    });
  }, [resync, setProject]);

  return { project, loadError, setProject, resync };
}
