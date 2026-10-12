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
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";
import type { DocumentDiff } from "./bindings/DocumentDiff";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { RecentProject } from "./bindings/RecentProject";
import { projectSnapshot } from "./testData";

type Args = Record<string, unknown>;
type Handlers = Record<string, (args: Args) => unknown>;

const recentProjects: RecentProject[] = [
  { path: "C:\\work\\amp", name: "amp", exists: true },
  { path: "D:\\old\\gone", name: "gone", exists: false },
];

const core: Handlers = {
  get_app_info: () => ({ name: "Tron", version: "0.1.0" }),
  get_project: () => null,
  list_recent_projects: () => [],
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
