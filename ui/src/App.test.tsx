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
import type { ErcRuleSetting } from "./bindings/ErcRuleSetting";
import type { ErcRuleSeverity } from "./bindings/ErcRuleSeverity";
import type { ErcViolation } from "./bindings/ErcViolation";
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
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
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
        "Symbol selected. Drag to move. R/X/Y: Transform. E: Edit. U/V: Edit text. Del: Remove",
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
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
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

describe("Wiring", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  });

  const START: [number, number] = [63_500_000, 50_800_000];
  const END: [number, number] = [76_200_000, 63_500_000];
  const PIN_1: [number, number] = [100_000_000, 46_190_000];

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

  function screenAt([x, y]: [number, number]) {
    const viewport = fitToSheet(CANVAS_SIZE, projectSnapshot().sheet);
    return worldToScreen(viewport, x, y);
  }

  function wireButton(): HTMLElement {
    return screen.getByRole("button", { name: "Draw wire" });
  }

  function optionNames(list: HTMLElement): (string | null)[] {
    const textOf = (element: HTMLElement) => element.textContent;
    return within(list).getAllByRole("option").map(textOf);
  }

  function argsOf(calls: { cmd: string; args: Args }[], cmd: string): Args[] {
    return calls.filter((call) => call.cmd === cmd).map((call) => call.args);
  }

  function clickAt(canvas: HTMLElement, world: [number, number]) {
    const at = screenAt(world);
    const position = { pointerId: 1, clientX: at.x, clientY: at.y };
    fireEvent.pointerMove(canvas, position);
    fireEvent.pointerDown(canvas, { ...position, button: 0 });
    fireEvent.pointerUp(canvas, { ...position, button: 0 });
  }

  async function openSheet(handlers: Handlers, snapshot = wiredSnapshot()) {
    const calls = mockCore({
      ...core,
      get_project: () => snapshot,
      ...handlers,
    });
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "amp" });
    // The canvas mounts before the resize effect fits the viewport; pointer
    // coordinates need the fitted viewport, so wait for the zoom readout.
    expect(await screen.findByText("Zoom 67%")).toBeTruthy();
    return { calls, canvas: canvasContainer() };
  }

  function wiredSnapshot(overrides: Partial<ProjectSnapshot> = {}) {
    return projectSnapshot({
      definitions: { "Device:R": resistorPreview() },
      instances: [resistorInstance()],
      ...overrides,
    });
  }

  it("draws an orthogonal wire with a bend and finishes on double-click", async () => {
    const { calls, canvas } = await openSheet({ draw_wire: () => null });

    fireEvent.keyDown(canvas, { key: "w" });
    expect(await screen.findByText("Wiring mode started")).toBeTruthy();
    expect(screen.getByText("Click to start wire. Esc: Exit")).toBeTruthy();
    expect(wireButton().getAttribute("aria-pressed")).toBe("true");

    clickAt(canvas, START);
    expect(
      screen.getByText(
        "Click to anchor. Double-click/Enter/Pin to finish. /: Toggle bend. Esc: Cancel",
      ),
    ).toBeTruthy();
    clickAt(canvas, END);
    clickAt(canvas, END);
    const at = screenAt(END);
    fireEvent.doubleClick(canvas, { clientX: at.x, clientY: at.y });

    await waitFor(() =>
      expect(calls.find((call) => call.cmd === "draw_wire")?.args).toEqual({
        points: [START, [END[0], START[1]], END],
      }),
    );
    expect(await screen.findByText("Wire completed")).toBeTruthy();
    expect(calls.filter((call) => call.cmd === "draw_wire")).toHaveLength(1);
  });

  it("toggles the bend and finishes on a pin end", async () => {
    const { calls, canvas } = await openSheet({ draw_wire: () => null });

    fireEvent.keyDown(canvas, { key: "w" });
    clickAt(canvas, START);
    fireEvent.keyDown(canvas, { key: "/" });
    clickAt(canvas, PIN_1);

    await waitFor(() =>
      expect(calls.find((call) => call.cmd === "draw_wire")?.args).toEqual({
        points: [START, [START[0], PIN_1[1]], PIN_1],
      }),
    );
  });

  it("cancels the wire on the first Escape and exits on the second", async () => {
    const { calls, canvas } = await openSheet({});

    fireEvent.click(wireButton());
    expect(await screen.findByText("Wiring mode started")).toBeTruthy();
    clickAt(canvas, START);

    fireEvent.keyDown(canvas, { key: "Escape" });
    expect(await screen.findByText("Wire cancelled")).toBeTruthy();
    expect(screen.getByText("Click to start wire. Esc: Exit")).toBeTruthy();

    fireEvent.keyDown(canvas, { key: "Escape" });
    expect(await screen.findByText("Wiring mode ended")).toBeTruthy();
    expect(wireButton().getAttribute("aria-pressed")).toBe("false");
    expect(calls.some((call) => call.cmd === "draw_wire")).toBe(false);
  });

  it("selects a wire and deletes it", async () => {
    const { calls, canvas } = await openSheet(
      { delete_wiring: () => null },
      wiredSnapshot({
        wires: [{ uuid: "wire-1", start: START, end: [END[0], START[1]] }],
      }),
    );
    expect(
      screen.getByText("1 wire, 0 junctions, 0 no-connect flags"),
    ).toBeTruthy();

    clickAt(canvas, [70_000_000, START[1]]);
    expect(await screen.findByText("Wire selected")).toBeTruthy();
    expect(
      screen.getByText("Wire selected. Drag to move. Del: Remove"),
    ).toBeTruthy();

    fireEvent.keyDown(canvas, { key: "Delete" });
    await waitFor(() =>
      expect(calls.find((call) => call.cmd === "delete_wiring")?.args).toEqual({
        uuids: ["wire-1"],
      }),
    );
    expect(await screen.findByText("Wire deleted")).toBeTruthy();
  });

  it("places local labels repeatedly with counting names", async () => {
    const { calls, canvas } = await openSheet({ place_label: () => null });

    fireEvent.keyDown(canvas, { key: "l" });
    const name = await screen.findByRole("textbox", {
      name: "Local label name",
    });
    expect(screen.getByText("Enter label name. Esc: Cancel")).toBeTruthy();

    fireEvent.change(name, { target: { value: "SDA 1" } });
    fireEvent.click(screen.getByRole("button", { name: "Place" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("Label names can't contain spaces.");

    fireEvent.change(name, { target: { value: "SDA1" } });
    fireEvent.click(screen.getByRole("button", { name: "Place" }));
    expect(await screen.findByText("Placing label SDA1")).toBeTruthy();
    expect(
      screen.getByText("Click to place. R: Rotate. Esc: Cancel"),
    ).toBeTruthy();

    fireEvent.keyDown(canvas, { key: "r" });
    clickAt(canvas, START);
    clickAt(canvas, END);

    await waitFor(() =>
      expect(argsOf(calls, "place_label")).toEqual([
        {
          label: {
            text: "SDA1",
            shape: null,
            xNm: START[0],
            yNm: START[1],
            rotation: 3,
          },
        },
        {
          label: {
            text: "SDA2",
            shape: null,
            xNm: END[0],
            yNm: END[1],
            rotation: 3,
          },
        },
      ]),
    );
    expect(await screen.findByText("Label placed")).toBeTruthy();
  });

  it("places a global label with the chosen shape", async () => {
    const { calls, canvas } = await openSheet({ place_label: () => null });

    fireEvent.keyDown(canvas, { key: "l", code: "KeyL", altKey: true });
    fireEvent.change(
      await screen.findByRole("textbox", { name: "Global label name" }),
      { target: { value: "VIN" } },
    );
    fireEvent.change(screen.getByRole("combobox", { name: "Shape" }), {
      target: { value: "output" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Place" }));
    expect(await screen.findByText("Placing label VIN")).toBeTruthy();
    clickAt(canvas, START);

    await waitFor(() =>
      expect(calls.find((call) => call.cmd === "place_label")?.args).toEqual({
        label: {
          text: "VIN",
          shape: "output",
          xNm: START[0],
          yNm: START[1],
          rotation: 0,
        },
      }),
    );
  });

  it("starts power symbol placement from the picker", async () => {
    const { calls } = await openSheet({
      search_library: ({ query }) => [
        {
          kind: "symbol",
          id: query === "GND" ? 41 : 42,
          name: query,
          lib: "power",
          description: "",
        },
      ],
      get_library_symbol: () => deviceRSymbol(),
    });

    fireEvent.click(screen.getByRole("button", { name: "Add power symbol" }));
    const picker = await screen.findByRole("listbox", {
      name: "Power symbols",
    });
    await waitFor(() =>
      expect(optionNames(picker)).toEqual([
        "GND",
        "+5V",
        "+3V3",
        "VCC",
        "Search library...",
      ]),
    );

    fireEvent.click(within(picker).getByRole("option", { name: "GND" }));
    expect(await screen.findByText("Placing GND")).toBeTruthy();
    await waitFor(() =>
      expect(
        calls.find((call) => call.cmd === "get_library_symbol")?.args,
      ).toEqual({ id: 41 }),
    );
  });

  it("asks for the library before offering power symbols", async () => {
    const { calls, canvas } = await openSheet({
      get_library_status: () => missingLibrary("declined"),
    });

    fireEvent.keyDown(canvas, { key: "p" });
    const picker = await screen.findByRole("dialog", { name: "Power symbols" });
    expect(within(picker).getByText("Component library required")).toBeTruthy();
    expect(
      within(picker).getByRole("button", { name: "Download" }),
    ).toBeTruthy();
    expect(calls.some((call) => call.cmd === "search_library")).toBe(false);
  });

  function noConnectButton(): HTMLElement {
    return screen.getByRole("button", { name: "Add no-connect flag" });
  }

  it("places no-connect flags on pins and leaves the mode with Escape", async () => {
    const { calls, canvas } = await openSheet({
      place_no_connect: () => null,
    });

    fireEvent.keyDown(canvas, { key: "q" });
    expect(await screen.findByText("Placing no-connect flag")).toBeTruthy();
    expect(screen.getByText("Click to place flag. Esc: Cancel")).toBeTruthy();
    expect(noConnectButton().getAttribute("aria-pressed")).toBe("true");

    clickAt(canvas, [PIN_1[0] + 200_000, PIN_1[1] + 200_000]);
    clickAt(canvas, START);
    await waitFor(() =>
      expect(argsOf(calls, "place_no_connect")).toEqual([
        { position: PIN_1 },
        { position: START },
      ]),
    );
    expect(await screen.findByText("Flag placed")).toBeTruthy();

    fireEvent.keyDown(canvas, { key: "Escape" });
    expect(await screen.findByText("No-connect mode ended")).toBeTruthy();
    expect(noConnectButton().getAttribute("aria-pressed")).toBe("false");
  });

  it("shows the core's reason when a flag can't be placed", async () => {
    const { canvas } = await openSheet({
      place_no_connect: () => {
        throw new Error("A no-connect flag is already there.");
      },
    });

    fireEvent.click(noConnectButton());
    expect(await screen.findByText("Placing no-connect flag")).toBeTruthy();
    clickAt(canvas, START);
    expect(
      await screen.findByText("A no-connect flag is already there."),
    ).toBeTruthy();
  });

  it("selects a no-connect flag and deletes it", async () => {
    const { calls, canvas } = await openSheet(
      { delete_wiring: () => null },
      wiredSnapshot({ noConnects: [{ uuid: "flag-1", position: START }] }),
    );
    expect(
      screen.getByText("0 wires, 0 junctions, 1 no-connect flag"),
    ).toBeTruthy();

    clickAt(canvas, START);
    expect(await screen.findByText("Flag selected")).toBeTruthy();
    expect(
      screen.getByText("Flag selected. Drag to move. Del: Remove"),
    ).toBeTruthy();

    fireEvent.keyDown(canvas, { key: "r" });
    fireEvent.keyDown(canvas, { key: "Delete" });
    await waitFor(() =>
      expect(argsOf(calls, "delete_wiring")).toEqual([{ uuids: ["flag-1"] }]),
    );
    expect(await screen.findByText("Flag deleted")).toBeTruthy();
    expect(calls.some((call) => call.cmd === "rotate_labels")).toBe(false);
  });

  const CENTER: [number, number] = [100_000_000, 50_000_000];

  function resistorFields() {
    return [
      { name: "Reference", value: "R?", visible: true },
      { name: "Value", value: "R", visible: true },
      { name: "Footprint", value: "", visible: false },
      { name: "Datasheet", value: "", visible: false },
    ];
  }

  it("edits symbol fields in the properties dialog", async () => {
    const { calls, canvas } = await openSheet({
      get_symbol_fields: () => resistorFields(),
      edit_symbol_fields: () => null,
    });

    clickAt(canvas, CENTER);
    expect(await screen.findByText("Symbol selected")).toBeTruthy();
    fireEvent.keyDown(canvas, { key: "e" });
    const dialog = await screen.findByRole("dialog", {
      name: "Symbol Properties",
    });
    const apply = within(dialog).getByRole("button", { name: "Apply" });
    expect(apply.hasAttribute("disabled")).toBe(true);

    const reference = within(dialog).getByRole("textbox", {
      name: "Reference value",
    });
    expect(document.activeElement).toBe(reference);
    fireEvent.change(reference, { target: { value: "" } });
    expect(within(dialog).getByRole("alert").textContent).toBe(
      "Reference cannot be empty.",
    );
    expect(apply.hasAttribute("disabled")).toBe(true);
    fireEvent.change(reference, { target: { value: "R3" } });
    fireEvent.click(
      within(dialog).getByRole("checkbox", { name: "Show Footprint" }),
    );

    fireEvent.click(within(dialog).getByRole("button", { name: "Add Field" }));
    const name = within(dialog).getByRole("textbox", {
      name: "Name of field 5",
    });
    expect(document.activeElement).toBe(name);
    const input = name as HTMLInputElement;
    expect(input.value).toBe("Field");
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(5);
    fireEvent.change(name, { target: { value: "value" } });
    expect(within(dialog).getByRole("alert").textContent).toBe(
      "Property names must be unique.",
    );
    expect(apply.hasAttribute("disabled")).toBe(true);
    fireEvent.change(name, { target: { value: "MPN" } });
    fireEvent.change(
      within(dialog).getByRole("textbox", { name: "MPN value" }),
      { target: { value: "RC0603" } },
    );
    fireEvent.click(apply);

    await waitFor(() =>
      expect(argsOf(calls, "edit_symbol_fields")).toEqual([
        {
          uuid: "symbol-1",
          fields: [
            { name: "Reference", value: "R3", visible: true },
            { name: "Value", value: "R", visible: true },
            { name: "Footprint", value: "", visible: true },
            { name: "Datasheet", value: "", visible: false },
            { name: "MPN", value: "RC0603", visible: true },
          ],
        },
      ]),
    );
    expect(await screen.findByText("Symbol properties updated")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("removes a custom field and opens on double-click", async () => {
    const { calls, canvas } = await openSheet({
      get_symbol_fields: () => [
        ...resistorFields(),
        { name: "MPN", value: "RC0603", visible: false },
      ],
      edit_symbol_fields: () => {
        throw new Error("The symbol is no longer on the schematic.");
      },
    });

    const at = screenAt(CENTER);
    fireEvent.doubleClick(canvas, { clientX: at.x, clientY: at.y });
    const dialog = await screen.findByRole("dialog", {
      name: "Symbol Properties",
    });
    expect(
      within(dialog).queryByRole("button", { name: "Remove Reference" }),
    ).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove MPN" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect((await within(dialog).findByRole("alert")).textContent).toBe(
      "The symbol is no longer on the schematic.",
    );
    expect(argsOf(calls, "edit_symbol_fields")[0]?.fields).toHaveLength(4);
  });

  it("edits the reference inline with U", async () => {
    const { calls, canvas } = await openSheet({
      set_symbol_field: () => null,
    });

    clickAt(canvas, CENTER);
    fireEvent.keyDown(canvas, { key: "u" });
    const input = await screen.findByRole("textbox", {
      name: "Edit Reference",
    });
    expect((input as HTMLInputElement).value).toBe("R?");
    fireEvent.change(input, { target: { value: " " } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Reference cannot be empty.",
    );

    fireEvent.change(input, { target: { value: "R5" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() =>
      expect(argsOf(calls, "set_symbol_field")).toEqual([
        { uuid: "symbol-1", name: "Reference", value: "R5" },
      ]),
    );
    expect(await screen.findByText("Symbol properties updated")).toBeTruthy();
    expect(
      screen.queryByRole("textbox", { name: "Edit Reference" }),
    ).toBeNull();
  });

  it("annotates the schematic from the Tools menu", async () => {
    let numbered = 2;
    const { calls } = await openSheet({ annotate_schematic: () => numbered });

    await emitFromCore("menu-action", "annotate-schematic");
    const annotated = await screen.findAllByText("Annotated 2 symbols");
    expect(annotated.length).toBeGreaterThan(0);

    numbered = 0;
    await emitFromCore("menu-action", "annotate-schematic");
    const done = await screen.findAllByText(
      "Schematic is already fully annotated",
    );
    expect(done.length).toBeGreaterThan(0);
    expect(argsOf(calls, "annotate_schematic")).toHaveLength(2);
  });

  function violation(overrides: Partial<ErcViolation> = {}): ErcViolation {
    return {
      id: "pin_not_connected:symbol-1:2",
      rule: "pin_not_connected",
      severity: "error",
      message: "Pin not connected",
      position: [100_000_000, 53_810_000],
      uuids: ["symbol-1"],
      items: ["R? pin 2 [Passive]"],
      ...overrides,
    };
  }

  function ercPanel(): Promise<HTMLElement> {
    return screen.findByRole("region", { name: "Electrical Rules Checker" });
  }

  it("runs ERC from the Tools menu and lists violations by severity", async () => {
    const snapshot = wiredSnapshot();
    const { calls, canvas } = await openSheet(
      {
        run_erc: () => [
          violation(),
          violation({
            id: "unannotated:symbol-1",
            rule: "unannotated",
            message: "Symbol is not annotated",
            position: [100_000_000, 50_000_000],
            items: ["Symbol R? [R]"],
          }),
          violation({
            id: "isolated_pin_label:label-1",
            rule: "isolated_pin_label",
            severity: "warning",
            message: "Label connected to only one pin",
            position: [63_500_000, 50_800_000],
            uuids: ["label-1"],
            items: ["Label OUT"],
          }),
        ],
      },
      snapshot,
    );

    await emitFromCore("menu-action", "electrical-rules-checker");
    const panel = await ercPanel();
    expect(
      within(panel).getByText(
        "Run ERC to check for electrical rules violations.",
      ),
    ).toBeTruthy();
    fireEvent.click(within(panel).getByRole("button", { name: "Run ERC" }));

    const list = await within(panel).findByRole("listbox", {
      name: "Violations",
    });
    expect(
      within(list).getByRole("group", { name: "Errors (2)" }),
    ).toBeTruthy();
    expect(
      within(list).getByRole("group", { name: "Warnings (1)" }),
    ).toBeTruthy();
    expect(
      await screen.findByText("ERC complete: 2 errors, 1 warning"),
    ).toBeTruthy();
    const options = within(list).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      "Pin not connectedR? pin 2 [Passive]",
      "Symbol is not annotatedSymbol R? [R]",
      "Label connected to only one pinLabel OUT",
    ]);
    const selected = () =>
      options.map((option) => option.getAttribute("aria-selected"));

    clickAt(canvas, [100_000_000, 53_810_000]);
    expect(selected()).toEqual(["true", "false", "false"]);
    expect(screen.queryByText("Symbol selected")).toBeNull();
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(selected()).toEqual(["false", "true", "false"]);
    fireEvent.click(options[2]!);
    expect(selected()).toEqual(["false", "false", "true"]);
    expect(argsOf(calls, "run_erc")).toHaveLength(1);

    await emitFromCore("document-changed", {
      baseRevision: snapshot.revision,
      revision: snapshot.revision + 1,
      titleBlock: null,
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
      history: { ...snapshot.history, dirty: true },
    });
    expect(await within(panel).findByText("Out of date")).toBeTruthy();
  });

  it("shows a clean result and core errors in the ERC panel", async () => {
    let failure: string | null = null;
    await openSheet({
      run_erc: () => {
        if (failure !== null) {
          throw new Error(failure);
        }
        return [];
      },
    });

    await emitFromCore("menu-action", "electrical-rules-checker");
    const panel = await ercPanel();
    fireEvent.click(within(panel).getByRole("button", { name: "Run ERC" }));
    expect(await within(panel).findByText("No violations found.")).toBeTruthy();
    expect(screen.getByText("ERC complete: No violations found")).toBeTruthy();

    failure = "R1 uses Device:R, which is not embedded in the schematic.";
    fireEvent.click(within(panel).getByRole("button", { name: "Run ERC" }));
    expect((await within(panel).findByRole("alert")).textContent).toBe(failure);

    fireEvent.click(
      within(panel).getByRole("button", {
        name: "Close Electrical Rules Checker",
      }),
    );
    expect(
      screen.queryByRole("region", { name: "Electrical Rules Checker" }),
    ).toBeNull();

    const toggle = screen.getByRole("button", {
      name: "Electrical Rules Checker",
    });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(within(await ercPanel()).getByRole("alert").textContent).toBe(
      failure,
    );
  });

  function ercRule(
    key: string,
    message: string,
    severity: ErcRuleSeverity,
    defaultSeverity: ErcRuleSeverity = severity,
  ): ErcRuleSetting {
    return { key, message, severity, defaultSeverity };
  }

  function severitySelect(dialog: HTMLElement, message: string) {
    return within(dialog).getByRole<HTMLSelectElement>("combobox", {
      name: `Severity for ${message}`,
    });
  }

  it("edits ERC rule severities and re-runs an open check", async () => {
    const snapshot = wiredSnapshot();
    const rules = [
      ercRule("pin_not_connected", "Pin not connected", "warning", "error"),
      ercRule("pin_to_pin", "Conflict problem between pins", "warning"),
    ];
    const { calls } = await openSheet(
      {
        run_erc: () => [],
        get_erc_rules: () => rules,
        edit_erc_severities: () => null,
      },
      snapshot,
    );

    await emitFromCore("menu-action", "electrical-rules-checker");
    const panel = await ercPanel();
    fireEvent.click(within(panel).getByRole("button", { name: "Run ERC" }));
    expect(await within(panel).findByText("No violations found.")).toBeTruthy();

    fireEvent.click(
      within(panel).getByRole("button", { name: "Rule Severities" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "ERC Rule Severities",
    });
    const pinSelect = await waitFor(() =>
      severitySelect(dialog, "Pin not connected"),
    );
    await waitFor(() => expect(document.activeElement).toBe(pinSelect));
    const matrix = severitySelect(dialog, "Conflict problem between pins");
    const choices = within(matrix).getAllByRole("option");
    expect(choices.map((choice) => choice.textContent)).toEqual([
      "Use Pin Conflict Matrix",
      "Ignore",
    ]);
    const apply = within(dialog).getByRole<HTMLButtonElement>("button", {
      name: "Apply",
    });
    expect(apply.disabled).toBe(true);

    fireEvent.change(matrix, { target: { value: "ignore" } });
    expect(apply.disabled).toBe(false);
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Reset to Defaults" }),
    );
    expect(pinSelect.value).toBe("error");
    expect(matrix.value).toBe("matrix");
    fireEvent.click(apply);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(argsOf(calls, "edit_erc_severities")).toEqual([
      { severities: { pin_not_connected: "error" } },
    ]);

    await emitFromCore("document-changed", {
      baseRevision: snapshot.revision,
      revision: snapshot.revision + 1,
      titleBlock: null,
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: true,
      simulationSettingsChanged: false,
      history: { ...snapshot.history, dirty: true, canUndo: true },
    });
    await waitFor(() => expect(argsOf(calls, "run_erc")).toHaveLength(2));
    await waitFor(() =>
      expect(within(panel).queryByText("Out of date")).toBeNull(),
    );
  });

  it("explains why ERC rule severities can't be loaded", async () => {
    const reason =
      "amp.kicad_pro can't be read, so its ERC settings can't be changed: EOF while parsing an object at line 1 column 10";
    await openSheet({
      get_erc_rules: () => {
        throw new Error(reason);
      },
    });

    await emitFromCore("menu-action", "erc-rule-severities");
    const dialog = await screen.findByRole("dialog", {
      name: "ERC Rule Severities",
    });
    expect((await within(dialog).findByRole("alert")).textContent).toBe(
      `Failed to load project settings: ${reason}`,
    );
    expect(within(dialog).queryByRole("button", { name: "Apply" })).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
