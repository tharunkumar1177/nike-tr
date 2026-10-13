import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { DocumentDiff } from "./bindings/DocumentDiff";
import type { LibraryHit } from "./bindings/LibraryHit";
import type { LibraryStatus } from "./bindings/LibraryStatus";
import type { LibrarySymbol } from "./bindings/LibrarySymbol";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { RecentProject } from "./bindings/RecentProject";
import { snapToGrid } from "./symbols";
import { projectSnapshot, resistorInstance, resistorPreview } from "./testData";
import { fitToSheet, pxPerNm, screenToWorld, worldToScreen } from "./viewport";

type Args = Record<string, unknown>;
type Handlers = Record<string, (args: Args) => unknown>;

const recentProjects: RecentProject[] = [
  { path: "C:\\work\\amp", name: "amp", exists: true },
  { path: "D:\\old\\gone", name: "gone", exists: false },
];

const ATTRIBUTION =
  "Symbols and footprints from the KiCad libraries (https://gitlab.com/kicad/libraries), licensed CC-BY-SA 4.0 with the KiCad libraries exception.";

const readyLibrary: LibraryStatus = {
  state: "ready",
  kicadTag: "10.0.7",
  symbolCount: 22_905,
  footprintCount: 15_464,
  attribution: ATTRIBUTION,
};

function missingLibrary(
  consent: "unasked" | "declined" | "accepted",
): LibraryStatus {
  return {
    state: "missing",
    kicadTag: "10.0.7",
    sizeBytes: 16_517_663,
    resumeBytes: 0,
    consent,
    error: null,
  };
}

const resistorHit: LibraryHit = {
  kind: "symbol",
  id: 7,
  name: "R",
  lib: "Device",
  description: "Resistor",
};

/** The library's Device:R entry. */
function deviceRSymbol(): LibrarySymbol {
  return {
    lib: "Device",
    name: "R",
    description: "Resistor",
    keywords: "R res resistor",
    datasheet: "",
    reference: "R",
    footprint: "",
    unitCount: 1,
    pinCount: 2,
    power: false,
    preview: resistorPreview(),
  };
}

/** The canvas size every observed element reports in these tests. */
const CANVAS_SIZE = { width: 800, height: 600 };

class ResizeObserverStub {
  private readonly callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
  }

  observe() {
    this.callback(
      [{ contentRect: CANVAS_SIZE } as unknown as ResizeObserverEntry],
      this as unknown as ResizeObserver,
    );
  }

  unobserve() {
    // Nothing to release in the stub.
  }

  disconnect() {
    // Nothing to release in the stub.
  }
}

const core: Handlers = {
  get_app_info: () => ({ name: "Tron", version: "0.1.0" }),
  get_project: () => null,
  list_recent_projects: () => [],
  get_library_status: () => readyLibrary,
};

function mockCore(handlers: Handlers) {
  const calls: { cmd: string; args: Args }[] = [];
  mockIPC(
    (cmd, args) => {
      const payload = (args ?? {}) as Args;
      calls.push({ cmd, args: payload });
      const handler = handlers[cmd];
      if (!handler) {
        throw new Error(`unexpected command ${cmd}`);
      }
      return handler(payload);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

async function emitFromCore(event: string, payload: unknown) {
  await act(async () => {
    await emit(event, payload);
  });
}

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

describe("App", () => {
  it("shows the welcome screen with recent projects", async () => {
    mockCore({ ...core, list_recent_projects: () => recentProjects });

    render(<App />);

    expect(
      await screen.findByRole("heading", { level: 1, name: "Tron" }),
    ).toBeTruthy();
    expect(await screen.findByRole("button", { name: /amp/ })).toBeTruthy();
    expect(screen.getByText("Missing")).toBeTruthy();
    expect(await screen.findByText("Tron 0.1.0")).toBeTruthy();
  });

  it("says so when there are no recent projects", async () => {
    mockCore(core);
    render(<App />);
    expect(await screen.findByText("No recent projects.")).toBeTruthy();
  });

  it("opens a recent project and follows document diffs", async () => {
    let current: ProjectSnapshot | null = null;
    const opened = projectSnapshot({ revision: 3 });
    const calls = mockCore({
      ...core,
      get_project: () => current,
      list_recent_projects: () => recentProjects,
      open_project: () => {
        current = opened;
        return opened;
      },
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /amp/ }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "amp" }),
    ).toBeTruthy();
    expect(calls.find((call) => call.cmd === "open_project")?.args).toEqual({
      path: "C:\\work\\amp",
    });
    expect(screen.getByText("Grid 1.27 mm")).toBeTruthy();
    const undoButton = screen.getByRole("button", { name: "Undo" });
    expect((undoButton as HTMLButtonElement).disabled).toBe(true);

    const diff: DocumentDiff = {
      baseRevision: 3,
      revision: 4,
      titleBlock: { ...opened.titleBlock, title: "Amp" },
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: null,
      junctions: null,
      labels: null,
      history: {
        dirty: true,
        canUndo: true,
        canRedo: false,
        undoLabel: "Project Properties",
        redoLabel: null,
      },
    };
    await emitFromCore("document-changed", diff);
    const enabledUndo = screen.getByRole("button", {
      name: "Undo Project Properties",
    });
    expect((enabledUndo as HTMLButtonElement).disabled).toBe(false);

    current = projectSnapshot({ revision: 9, gridSizeNm: 2_540_000 });
    await emitFromCore("document-changed", { ...diff, baseRevision: 1 });
    expect(await screen.findByText("Grid 2.54 mm")).toBeTruthy();
  });

  it("asks to save before closing a project with unsaved changes", async () => {
    let current: ProjectSnapshot | null = projectSnapshot({
      history: {
        dirty: true,
        canUndo: true,
        canRedo: false,
        undoLabel: "Project Properties",
        redoLabel: null,
      },
    });
    const calls = mockCore({
      ...core,
      get_project: () => current,
      close_project: () => {
        current = null;
        return null;
      },
    });

    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "amp" });
    await emitFromCore("menu-action", "close-project");

    const dialog = await screen.findByRole("dialog", {
      name: "Save changes to amp?",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Don't Save" }));

    expect(await screen.findByText("No recent projects.")).toBeTruthy();
    expect(calls.some((call) => call.cmd === "close_project")).toBe(true);
    expect(calls.some((call) => call.cmd === "save_project")).toBe(false);
  });

  it("sends title block edits as one properties command", async () => {
    const calls = mockCore({
      ...core,
      get_project: () => projectSnapshot(),
      edit_project_properties: () => null,
    });

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Project Properties" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Project Properties",
    });
    fireEvent.change(within(dialog).getByLabelText("Title"), {
      target: { value: "Class A amplifier" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const edit = calls.find((call) => call.cmd === "edit_project_properties");
    expect(edit?.args).toEqual({
      properties: {
        titleBlock: {
          ...projectSnapshot().titleBlock,
          title: "Class A amplifier",
        },
        gridSizeNm: 1_270_000,
      },
    });
  });

  it("offers to remove a missing recent project", async () => {
    const calls = mockCore({
      ...core,
      list_recent_projects: () => recentProjects,
      remove_recent_project: () => recentProjects.slice(0, 1),
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /gone/ }));
    const dialog = await screen.findByRole("dialog", {
      name: "Project not found",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /gone/ })).toBeNull(),
    );
    const removal = calls.find((call) => call.cmd === "remove_recent_project");
    expect(removal?.args).toEqual({ path: "D:\\old\\gone" });
  });

  it("shows a core connection error and recovers on retry", async () => {
    let attempts = 0;
    mockCore({
      ...core,
      get_project: () => {
        attempts += 1;
        if (attempts === 1) {
          throw new Error("core unavailable");
        }
        return null;
      },
    });

    render(<App />);

    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Couldn't connect to the Tron core.",
      }),
    ).toBeTruthy();
    expect(screen.getByText("core unavailable")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("No recent projects.")).toBeTruthy();
  });
});

describe("Component library", () => {
  it("offers the download on first launch and records a decline", async () => {
    const calls = mockCore({
      ...core,
      get_library_status: () => missingLibrary("unasked"),
      decline_library_download: () => missingLibrary("declined"),
    });

    render(<App />);
    const dialog = await screen.findByRole("dialog", {
      name: "Component Library",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Decline" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls.some((call) => call.cmd === "decline_library_download")).toBe(
      true,
    );
  });

  it("closes the offer on Escape without recording an answer", async () => {
    const calls = mockCore({
      ...core,
      get_library_status: () => missingLibrary("unasked"),
    });

    render(<App />);
    const dialog = await screen.findByRole("dialog", {
      name: "Component Library",
    });
    fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls.some((call) => call.cmd.endsWith("library_download"))).toBe(
      false,
    );
  });

  it("does not offer the download again once the user answered", async () => {
    mockCore({
      ...core,
      get_library_status: () => missingLibrary("declined"),
    });

    render(<App />);
    expect(await screen.findByText("No recent projects.")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows download progress and closes when the library is ready", async () => {
    const calls = mockCore({
      ...core,
      get_library_status: () => missingLibrary("unasked"),
      download_library: () => ({
        state: "downloading",
        kicadTag: "10.0.7",
        downloadedBytes: 0,
        totalBytes: 16_517_663,
      }),
    });

    render(<App />);
    const offer = await screen.findByRole("dialog", {
      name: "Component Library",
    });
    fireEvent.click(within(offer).getByRole("button", { name: "Download" }));

    await screen.findByRole("dialog", { name: "Downloading Library..." });
    await emitFromCore("library-status", {
      state: "downloading",
      kicadTag: "10.0.7",
      downloadedBytes: 8_258_832,
      totalBytes: 16_517_663,
    });
    expect(
      screen.getByText("Downloading component index... (8.3 MB / 16.5 MB)"),
    ).toBeTruthy();
    expect(screen.getByText("Downloaded 50%")).toBeTruthy();

    await emitFromCore("library-status", readyLibrary);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls.some((call) => call.cmd === "download_library")).toBe(true);
  });

  it("explains a failed download and retries it", async () => {
    const calls = mockCore({
      ...core,
      get_library_status: () => missingLibrary("accepted"),
      get_project: () => projectSnapshot(),
      download_library: () => ({
        state: "downloading",
        kicadTag: "10.0.7",
        downloadedBytes: 4_000_000,
        totalBytes: 16_517_663,
      }),
    });

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Component Library" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Download Library" }));
    await screen.findByRole("dialog", { name: "Component Library" });

    await emitFromCore("library-status", {
      ...missingLibrary("accepted"),
      resumeBytes: 4_000_000,
      error: {
        kind: "network",
        message: "Could not download the component library.",
        detail: "connection reset",
      },
    });
    const failed = await screen.findByRole("dialog", {
      name: "Download Failed",
    });
    expect(
      within(failed).getByText("A network or disk error occurred."),
    ).toBeTruthy();
    expect(within(failed).getByText("connection reset")).toBeTruthy();

    fireEvent.click(within(failed).getByRole("button", { name: "Retry" }));
    await screen.findByRole("dialog", { name: "Downloading Library..." });
    expect(
      calls.filter((call) => call.cmd === "download_library"),
    ).toHaveLength(1);
  });

  it("searches the library and previews the selected symbol", async () => {
    const hits: LibraryHit[] = [
      resistorHit,
      {
        kind: "footprint",
        id: 3,
        name: "R_0603_1608Metric",
        lib: "Resistor_SMD",
        description: "Resistor SMD 0603 (1608 Metric)",
      },
    ];
    const calls = mockCore({
      ...core,
      get_project: () => projectSnapshot(),
      search_library: () => hits,
      get_library_symbol: () => deviceRSymbol(),
    });

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Component Library" }),
    );
    expect(screen.getByText(ATTRIBUTION)).toBeTruthy();
    fireEvent.change(
      screen.getByRole("searchbox", { name: "Search symbols and footprints" }),
      { target: { value: " R " } },
    );

    const options = await screen.findAllByRole("option");
    expect(options).toHaveLength(2);
    expect(within(options[1]!).getByText("Footprint")).toBeTruthy();
    expect(calls.find((call) => call.cmd === "search_library")?.args).toEqual({
      query: "R",
      limit: 200,
    });

    fireEvent.click(options[0]!);
    expect(
      await screen.findByRole("img", { name: "Symbol Device:R" }),
    ).toBeTruthy();
    expect(
      calls.find((call) => call.cmd === "get_library_symbol")?.args,
    ).toEqual({ id: 7 });
  });

  it("says when a search has no matches", async () => {
    mockCore({
      ...core,
      get_project: () => projectSnapshot(),
      search_library: () => [],
    });

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Component Library" }),
    );
    fireEvent.change(
      screen.getByRole("searchbox", { name: "Search symbols and footprints" }),
      { target: { value: "zzzz" } },
    );
    expect(
      await screen.findByText('No matches found for "zzzz".'),
    ).toBeTruthy();
  });
});

describe("Symbol placement", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  });

  function canvasContainer(): HTMLElement {
    const surface = screen.getByRole("img", {
      name: "Schematic sheet of amp",
    });
    const container = surface.parentElement;
    if (!container) {
      throw new Error("canvas container not found");
    }
    return container;
  }

  function fittedViewport() {
    return fitToSheet(CANVAS_SIZE, projectSnapshot().sheet);
  }

  /** Lets the definition load triggered by starting placement settle. */
  async function flushCore() {
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
  }

  async function searchForResistor(): Promise<HTMLElement> {
    fireEvent.click(
      await screen.findByRole("button", { name: "Component Library" }),
    );
    fireEvent.change(
      screen.getByRole("searchbox", { name: "Search symbols and footprints" }),
      { target: { value: "R" } },
    );
    const options = await screen.findAllByRole("option");
    const first = options[0];
    if (!first) {
      throw new Error("no search results");
    }
    return first;
  }

  function placementCore(): { cmd: string; args: Args }[] {
    return mockCore({
      ...core,
      get_project: () => projectSnapshot(),
      search_library: () => [resistorHit],
      get_library_symbol: () => deviceRSymbol(),
      place_symbol: () => null,
    });
  }

  it("places a symbol from the browser at the snapped cursor position", async () => {
    const calls = placementCore();

    render(<App />);
    const option = await searchForResistor();
    fireEvent.doubleClick(option);

    expect(await screen.findByText("Placing R")).toBeTruthy();
    expect(
      screen.getByText("Click to place. R: Rotate, X/Y: Mirror. Esc: Cancel"),
    ).toBeTruthy();
    await flushCore();

    const canvas = canvasContainer();
    expect(document.activeElement).toBe(canvas);
    expect(
      calls.find((call) => call.cmd === "get_library_symbol")?.args,
    ).toEqual({ id: 7 });

    const at = worldToScreen(fittedViewport(), 63_510_000, 50_810_000);
    fireEvent.pointerDown(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });
    fireEvent.pointerUp(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });

    await waitFor(() =>
      expect(calls.find((call) => call.cmd === "place_symbol")?.args).toEqual({
        placement: {
          libraryId: 7,
          xNm: 63_500_000,
          yNm: 50_800_000,
          rotation: 0,
          mirror: null,
        },
      }),
    );
    expect(await screen.findByText("Symbol placed")).toBeTruthy();
  });

  it("starts placement with Enter and sends the R rotation", async () => {
    const calls = placementCore();

    render(<App />);
    const option = await searchForResistor();
    fireEvent.click(option);
    fireEvent.keyDown(option, { key: "Enter" });

    expect(await screen.findByText("Placing R")).toBeTruthy();
    await flushCore();

    const canvas = canvasContainer();
    fireEvent.keyDown(canvas, { key: "r" });
    fireEvent.keyDown(canvas, { key: "Enter" });

    const center = screenToWorld(fittedViewport(), 400, 300);
    await waitFor(() => {
      const place = calls.find((call) => call.cmd === "place_symbol");
      expect(place?.args).toEqual({
        placement: {
          libraryId: 7,
          xNm: snapToGrid(center.x, 1_270_000),
          yNm: snapToGrid(center.y, 1_270_000),
          rotation: 3,
          mirror: null,
        },
      });
    });
  });

  it("exits placement on Escape without placing", async () => {
    const calls = placementCore();

    render(<App />);
    const option = await searchForResistor();
    fireEvent.doubleClick(option);
    expect(await screen.findByText("Placing R")).toBeTruthy();
    await flushCore();

    const canvas = canvasContainer();
    fireEvent.keyDown(canvas, { key: "Escape" });
    expect(
      screen.queryByText("Click to place. R: Rotate, X/Y: Mirror. Esc: Cancel"),
    ).toBeNull();

    const at = worldToScreen(fittedViewport(), 63_500_000, 50_800_000);
    fireEvent.pointerDown(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });
    fireEvent.pointerUp(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });
    expect(calls.some((call) => call.cmd === "place_symbol")).toBe(false);
  });

  it("selects a placed symbol and deletes it", async () => {
    const calls = mockCore({
      ...core,
      get_project: () =>
        projectSnapshot({
          definitions: { "Device:R": resistorPreview() },
          instances: [resistorInstance()],
        }),
      delete_symbols: () => null,
    });

    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "amp" });
    expect(await screen.findByText("R? Device:R")).toBeTruthy();
    // The canvas mounts before the resize effect fits the viewport; pointer
    // coordinates need the fitted viewport, so wait for the zoom readout.
    expect(await screen.findByText("Zoom 67%")).toBeTruthy();

    const canvas = canvasContainer();
    const at = worldToScreen(fittedViewport(), 100_000_000, 50_000_000);
    fireEvent.pointerDown(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });
    fireEvent.pointerUp(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });

    expect(await screen.findByText("Symbol selected")).toBeTruthy();
    expect(
      screen.getByText(
        "Symbol selected. Drag to move. R/X/Y: Transform. Del: Remove",
      ),
    ).toBeTruthy();

    fireEvent.keyDown(canvas, { key: "Delete" });
    await waitFor(() =>
      expect(calls.find((call) => call.cmd === "delete_symbols")?.args).toEqual(
        { uuids: ["symbol-1"] },
      ),
    );
    expect(await screen.findByText("Symbol deleted")).toBeTruthy();
  });

  it("moves a dragged symbol with one command on release", async () => {
    const calls = mockCore({
      ...core,
      get_project: () =>
        projectSnapshot({
          definitions: { "Device:R": resistorPreview() },
          instances: [resistorInstance()],
        }),
      move_symbols: () => null,
    });

    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "amp" });
    // The canvas mounts before the resize effect fits the viewport; pointer
    // coordinates need the fitted viewport, so wait for the zoom readout.
    expect(await screen.findByText("Zoom 67%")).toBeTruthy();

    const canvas = canvasContainer();
    const viewport = fittedViewport();
    const at = worldToScreen(viewport, 100_000_000, 50_000_000);
    fireEvent.pointerDown(canvas, {
      button: 0,
      pointerId: 1,
      clientX: at.x,
      clientY: at.y,
    });
    fireEvent.pointerMove(canvas, {
      pointerId: 1,
      clientX: at.x + 32,
      clientY: at.y,
      buttons: 1,
    });
    fireEvent.pointerUp(canvas, {
      pointerId: 1,
      clientX: at.x + 32,
      clientY: at.y,
    });

    await waitFor(() =>
      expect(calls.filter((call) => call.cmd === "move_symbols")).toHaveLength(
        1,
      ),
    );
    const scale = pxPerNm(viewport.zoom);
    expect(calls.find((call) => call.cmd === "move_symbols")?.args).toEqual({
      uuids: ["symbol-1"],
      dxNm: snapToGrid(32 / scale, 1_270_000),
      dyNm: 0,
    });
  });

  it("renders symbols that arrive in a document diff", async () => {
    const snapshot = projectSnapshot();
    mockCore({ ...core, get_project: () => snapshot });

    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "amp" });

    const diff: DocumentDiff = {
      baseRevision: snapshot.revision,
      revision: snapshot.revision + 1,
      titleBlock: null,
      gridSizeNm: null,
      definitions: { "Device:R": resistorPreview() },
      instances: [resistorInstance()],
      wires: null,
      junctions: null,
      labels: null,
      history: {
        ...snapshot.history,
        dirty: true,
        canUndo: true,
        undoLabel: "Place Symbol",
      },
    };
    await emitFromCore("document-changed", diff);

    expect(await screen.findByText("R? Device:R")).toBeTruthy();
  });
});
