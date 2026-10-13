import type { ReactNode } from "react";

function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      className="icon"
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function UndoIcon() {
  return (
    <Icon>
      <path d="M9 14 4 9l5-5" />
      <path d="M4 9h11a5 5 0 0 1 0 10h-3" />
    </Icon>
  );
}

export function RedoIcon() {
  return (
    <Icon>
      <path d="m15 14 5-5-5-5" />
      <path d="M20 9H9a5 5 0 0 0 0 10h3" />
    </Icon>
  );
}

export function LibraryIcon() {
  return (
    <Icon>
      <rect x="3" y="4" width="5" height="16" rx="1" />
      <rect x="10" y="4" width="5" height="16" rx="1" />
      <path d="m17 5 3.5 14.5" />
    </Icon>
  );
}

export function WireIcon() {
  return (
    <Icon>
      <path d="M4 18h6V6h10" />
      <circle cx="4" cy="18" r="1.5" />
      <circle cx="20" cy="6" r="1.5" />
    </Icon>
  );
}

export function LabelIcon() {
  return (
    <Icon>
      <path d="M3 17h18" />
      <path d="M6 13V7h4" />
      <path d="M8 7v6" />
      <rect x="2" y="16" width="2" height="2" />
    </Icon>
  );
}

export function GlobalLabelIcon() {
  return (
    <Icon>
      <path d="M3 12 7 7h13v10H7z" />
      <path d="M10 12h7" />
    </Icon>
  );
}

export function RemoveIcon() {
  return (
    <Icon>
      <path d="M6 6l12 12" />
      <path d="M6 18L18 6" />
    </Icon>
  );
}

export function NoConnectIcon() {
  return (
    <Icon>
      <path d="M3 12h12" />
      <path d="M10 7l10 10" />
      <path d="M10 17L20 7" />
    </Icon>
  );
}

export function PowerIcon() {
  return (
    <Icon>
      <path d="M12 21V9" />
      <path d="M6 9h12" />
      <path d="M12 3v3" />
    </Icon>
  );
}

export function PropertiesIcon() {
  return (
    <Icon>
      <rect x="5" y="3" width="14" height="18" rx="2" />
      <path d="M9 8h6" />
      <path d="M9 12h6" />
      <path d="M9 16h3" />
    </Icon>
  );
}

export function ErcIcon() {
  return (
    <Icon>
      <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
      <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
      <path d="m9 14 2 2 4-4" />
    </Icon>
  );
}

export function ErrorIcon() {
  return (
    <Icon>
      <circle cx="12" cy="12" r="9" />
      <path d="m15 9-6 6" />
      <path d="m9 9 6 6" />
    </Icon>
  );
}

export function WarningIcon() {
  return (
    <Icon>
      <path d="M12 3 2 20h20L12 3Z" />
      <path d="M12 10v4" />
      <path d="M12 17h.01" />
    </Icon>
  );
}

export function CheckIcon() {
  return (
    <Icon>
      <path d="m5 12 5 5L20 7" />
    </Icon>
  );
}
