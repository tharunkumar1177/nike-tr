/**
 * Rules for editing a placed symbol's fields, matching the core: the
 * mandatory fields, length limits, and what makes a set of fields invalid.
 */
import type { SymbolField } from "./bindings/SymbolField";

/** The fields every placed symbol has, in the order KiCad lists them. */
export const MANDATORY_FIELDS: readonly string[] = [
  "Reference",
  "Value",
  "Footprint",
  "Datasheet",
];
export const MAX_FIELD_NAME_CHARS = 256;
export const MAX_FIELD_VALUE_CHARS = 4096;

export function isMandatoryField(name: string): boolean {
  return MANDATORY_FIELDS.includes(name);
}

/** Why a set of fields can't be applied, or `null` when it can. */
export function fieldsError(fields: SymbolField[]): string | null {
  const reference = fields.find((field) => field.name === "Reference");
  if (reference && reference.value.trim() === "") {
    return "Reference cannot be empty.";
  }
  const seen = new Set<string>();
  for (const field of fields) {
    if (field.name.trim() === "") {
      return "Custom field names cannot be empty.";
    }
    const key = field.name.toLowerCase();
    if (seen.has(key)) {
      return "Property names must be unique.";
    }
    seen.add(key);
  }
  return null;
}

export function sameFields(a: SymbolField[], b: SymbolField[]): boolean {
  return (
    a.length === b.length &&
    a.every((field, index) => {
      const other = b[index];
      return (
        other !== undefined &&
        other.name === field.name &&
        other.value === field.value &&
        other.visible === field.visible
      );
    })
  );
}
