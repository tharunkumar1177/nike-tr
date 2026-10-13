import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppErrorKind } from "./bindings/AppErrorKind";
import type { AppInfo } from "./bindings/AppInfo";
import type { LibraryFootprint } from "./bindings/LibraryFootprint";
import type { LibraryHit } from "./bindings/LibraryHit";
import type { LibraryStatus } from "./bindings/LibraryStatus";
import type { LibrarySymbol } from "./bindings/LibrarySymbol";
import type { ProjectProperties } from "./bindings/ProjectProperties";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { RecentProject } from "./bindings/RecentProject";
import type { SymbolPlacement } from "./bindings/SymbolPlacement";
import type { SymbolTransform } from "./bindings/SymbolTransform";

export const DOCUMENT_CHANGED_EVENT = "document-changed";
export const MENU_EVENT = "menu-action";
export const CLOSE_REQUESTED_EVENT = "close-requested";
export const OPEN_RECENT_PREFIX = "open-recent:";

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export function getProject(): Promise<ProjectSnapshot | null> {
  return invoke<ProjectSnapshot | null>("get_project");
}

export function createProject(
  parent: string,
  name: string,
): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("create_project", { parent, name });
}

export function openProject(path: string): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("open_project", { path });
}

export function saveProject(): Promise<void> {
  return invoke<void>("save_project");
}

export function saveProjectAs(
  parent: string,
  name: string,
): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("save_project_as", { parent, name });
}

export function closeProject(): Promise<void> {
  return invoke<void>("close_project");
}

export function editProjectProperties(
  properties: ProjectProperties,
): Promise<void> {
  return invoke<void>("edit_project_properties", { properties });
}

export function undo(): Promise<void> {
  return invoke<void>("undo");
}

export function redo(): Promise<void> {
  return invoke<void>("redo");
}

export function placeSymbol(placement: SymbolPlacement): Promise<void> {
  return invoke<void>("place_symbol", { placement });
}

export function moveSymbols(
  uuids: string[],
  dxNm: number,
  dyNm: number,
): Promise<void> {
  return invoke<void>("move_symbols", { uuids, dxNm, dyNm });
}

export function transformSymbols(
  uuids: string[],
  transform: SymbolTransform,
): Promise<void> {
  return invoke<void>("transform_symbols", { uuids, transform });
}

export function deleteSymbols(uuids: string[]): Promise<void> {
  return invoke<void>("delete_symbols", { uuids });
}

export function listRecentProjects(): Promise<RecentProject[]> {
  return invoke<RecentProject[]>("list_recent_projects");
}

export function removeRecentProject(path: string): Promise<RecentProject[]> {
  return invoke<RecentProject[]>("remove_recent_project", { path });
}

export function closeWindow(): Promise<void> {
  return invoke<void>("close_window");
}

export const LIBRARY_STATUS_EVENT = "library-status";
export const MAX_LIBRARY_RESULTS = 200;

export function getLibraryStatus(): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("get_library_status");
}

export function downloadLibrary(): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("download_library");
}

export function declineLibraryDownload(): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("decline_library_download");
}

export function cancelLibraryDownload(): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("cancel_library_download");
}

export function searchLibrary(
  query: string,
  limit: number = MAX_LIBRARY_RESULTS,
): Promise<LibraryHit[]> {
  return invoke<LibraryHit[]>("search_library", { query, limit });
}

export function getLibrarySymbol(id: number): Promise<LibrarySymbol | null> {
  return invoke<LibrarySymbol | null>("get_library_symbol", { id });
}

export function getLibraryFootprint(
  id: number,
): Promise<LibraryFootprint | null> {
  return invoke<LibraryFootprint | null>("get_library_footprint", { id });
}

/**
 * Listens for a core event until the returned function is called. Safe to
 * call from an effect: unsubscribing before `listen` resolves still detaches.
 */
export function subscribe<T>(
  event: string,
  handler: (payload: T) => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  listen<T>(event, (message) => handler(message.payload)).then(
    (detach) => {
      if (disposed) {
        detach();
      } else {
        unlisten = detach;
      }
    },
    (error: unknown) => {
      console.error(
        `Could not listen for ${event}: ${describeIpcError(error)}`,
      );
    },
  );
  return () => {
    disposed = true;
    unlisten?.();
  };
}

export interface ErrorReport {
  kind: AppErrorKind | null;
  message: string;
  detail: string | null;
}

export function toErrorReport(error: unknown): ErrorReport {
  if (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    typeof error.kind === "string" &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    const detail =
      "detail" in error && typeof error.detail === "string"
        ? error.detail
        : null;
    return {
      kind: error.kind as AppErrorKind,
      message: error.message,
      detail,
    };
  }
  return { kind: null, message: describeIpcError(error), detail: null };
}

export function describeIpcError(error: unknown): string {
  if (typeof error === "string" && error.length > 0) {
    return error;
  }
  if (error instanceof Error && error.message.length > 0) {
    return error.message;
  }
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string" &&
    error.message.length > 0
  ) {
    return error.message;
  }
  return "The desktop core returned an error without a message.";
}
