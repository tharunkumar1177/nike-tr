/**
 * Finds what the probe tool points at: a symbol pin, a wire, or a label
 * measure the voltage of their net, and a symbol body the current through it.
 */
import type { NetLabel } from "./bindings/NetLabel";
import type { ProbeTarget } from "./bindings/ProbeTarget";
import type { SymbolInstance } from "./bindings/SymbolInstance";
import type { SymbolPreview } from "./bindings/SymbolPreview";
import type { WireSegment } from "./bindings/WireSegment";
import type { Bounds } from "./libraryPreview";
import { findInstanceAt, instanceBounds, instancePose } from "./symbols";
import type { ScreenPoint } from "./viewport";
import { findLabelAt, findWireAt, labelBounds } from "./wiring";

/** The hover outline of a probe target, in world nanometers. */
export type ProbeOutline =
  | { shape: "circle"; center: ScreenPoint }
  | { shape: "segment"; start: ScreenPoint; end: ScreenPoint }
  | { shape: "box"; bounds: Bounds };

export interface ProbeHit {
  target: ProbeTarget;
  outline: ProbeOutline;
}

export interface ProbeScene {
  instances: SymbolInstance[];
  definitions: { [key in string]: SymbolPreview };
  wires: WireSegment[];
  labels: NetLabel[];
}

/** The topmost pin within `toleranceNm` of `point`. */
function findPinAt(
  instances: SymbolInstance[],
  point: ScreenPoint,
  toleranceNm: number,
): ProbeHit | null {
  for (let index = instances.length - 1; index >= 0; index--) {
    const instance = instances[index];
    const pin = instance?.pins.find(
      ([x, y]) => Math.hypot(point.x - x, point.y - y) <= toleranceNm,
    );
    if (instance && pin) {
      return {
        target: { kind: "pin", symbol: instance.uuid, position: pin },
        outline: { shape: "circle", center: { x: pin[0], y: pin[1] } },
      };
    }
  }
  return null;
}

/** What a probe click at `point` measures: pins first, then wires, labels, symbols. */
export function findProbeAt(
  scene: ProbeScene,
  point: ScreenPoint,
  toleranceNm: number,
): ProbeHit | null {
  const pin = findPinAt(scene.instances, point, toleranceNm);
  if (pin) {
    return pin;
  }
  const wire = findWireAt(scene.wires, point, toleranceNm);
  if (wire) {
    return {
      target: { kind: "item", uuid: wire.uuid },
      outline: {
        shape: "segment",
        start: { x: wire.start[0], y: wire.start[1] },
        end: { x: wire.end[0], y: wire.end[1] },
      },
    };
  }
  const label = findLabelAt(scene.labels, point);
  if (label) {
    return {
      target: { kind: "item", uuid: label.uuid },
      outline: { shape: "box", bounds: labelBounds(label) },
    };
  }
  const symbol = findInstanceAt(scene.instances, scene.definitions, point);
  const preview = symbol ? scene.definitions[symbol.libId] : undefined;
  const bounds =
    symbol && preview ? instanceBounds(preview, instancePose(symbol)) : null;
  if (symbol && bounds) {
    return {
      target: { kind: "item", uuid: symbol.uuid },
      outline: { shape: "box", bounds },
    };
  }
  return null;
}
