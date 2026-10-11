import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

const appInfo = { name: "Tron", version: "0.1.0" };

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("App", () => {
  it("shows the name and version reported by the core", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_app_info") {
        return appInfo;
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    render(<App />);

    expect(screen.getByText("Connecting to core...")).toBeTruthy();
    expect(
      await screen.findByRole("heading", { level: 1, name: "Tron" }),
    ).toBeTruthy();
    expect(screen.getByText("0.1.0")).toBeTruthy();
  });

  it("shows the core error and recovers on retry", async () => {
    let calls = 0;
    mockIPC((cmd) => {
      if (cmd !== "get_app_info") {
        throw new Error(`unexpected command ${cmd}`);
      }
      calls += 1;
      if (calls === 1) {
        throw new Error("core unavailable");
      }
      return appInfo;
    });

    render(<App />);

    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Failed to load app info.",
      }),
    ).toBeTruthy();
    expect(screen.getByText("core unavailable")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Tron" }),
    ).toBeTruthy();
    expect(calls).toBe(2);
  });
});
