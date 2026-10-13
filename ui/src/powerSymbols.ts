/** The power symbols the canvas offers for quick placement. */
import type { LibraryHit } from "./bindings/LibraryHit";
import { searchLibrary } from "./ipc";

/** The power symbols offered first, by name in the `power` library. */
export const COMMON_POWER_SYMBOLS = ["GND", "+5V", "+3V3", "VCC"];

const POWER_LIBRARY = "power";

function isPowerSymbol(hit: LibraryHit, name: string): boolean {
  return (
    hit.kind === "symbol" && hit.lib === POWER_LIBRARY && hit.name === name
  );
}

/** Finds each common power symbol in the library, in offer order. */
export async function findPowerSymbols(): Promise<LibraryHit[]> {
  const results = await Promise.all(
    COMMON_POWER_SYMBOLS.map(async (name) => {
      const hits = await searchLibrary(name);
      const hit = hits.find((candidate) => isPowerSymbol(candidate, name));
      return hit ?? null;
    }),
  );
  return results.filter((hit): hit is LibraryHit => hit !== null);
}
