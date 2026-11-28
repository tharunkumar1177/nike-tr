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

export function ProbeIcon() {
  return (
    <Icon>
      <path d="M14 4l6 6" />
      <path d="M17 7l-8 8-3-3 8-8" />
      <path d="M7.5 13.5 3 20" />
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

export function InfoIcon() {
  return (
    <Icon>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5" />
      <path d="M12 8h.01" />
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

export function PlayIcon() {
  return (
    <Icon>
      <path d="M7 4v16l13-8L7 4Z" />
    </Icon>
  );
}

export function StopIcon() {
  return (
    <Icon>
      <rect x="6" y="6" width="12" height="12" rx="1" />
    </Icon>
  );
}

export function SettingsIcon() {
  return (
    <Icon>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1" />
    </Icon>
  );
}

export function EyeIcon() {
  return (
    <Icon>
      <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z" />
      <circle cx="12" cy="12" r="3" />
    </Icon>
  );
}

/** A board with an arrow coming in: Update Board from Schematic. */
export function BoardUpdateIcon() {
  return (
    <Icon>
      <rect x="9" y="4" width="12" height="16" rx="1" />
      <path d="M2 12h9" />
      <path d="m8 9 3 3-3 3" />
      <circle cx="16" cy="9" r="1" />
      <circle cx="16" cy="15" r="1" />
    </Icon>
  );
}

/** A rectangle with handles on its corners: the board outline. */
export function BoardOutlineIcon() {
  return (
    <Icon>
      <rect x="5" y="6" width="14" height="12" />
      <circle cx="5" cy="6" r="1.5" />
      <circle cx="19" cy="6" r="1.5" />
      <circle cx="19" cy="18" r="1.5" />
      <circle cx="5" cy="18" r="1.5" />
    </Icon>
  );
}

/** Three stacked bars: the layer stackup. */
export function StackupIcon() {
  return (
    <Icon>
      <rect x="4" y="5" width="16" height="3" rx="0.5" />
      <rect x="4" y="10.5" width="16" height="3" rx="0.5" />
      <rect x="4" y="16" width="16" height="3" rx="0.5" />
    </Icon>
  );
}

/** A track bending at 45 degrees between two pads: route a track. */
export function RouteIcon() {
  return (
    <Icon>
      <circle cx="5" cy="18" r="2" />
      <circle cx="19" cy="6" r="2" />
      <path d="M7 18h4l6.6-6.6V8" />
    </Icon>
  );
}

/** Two offset sheets: the copper layers. */
export function LayersIcon() {
  return (
    <Icon>
      <path d="m12 4 9 5-9 5-9-5 9-5Z" />
      <path d="m3 14 9 5 9-5" />
    </Icon>
  );
}

/** A ruler: the design rules. */
export function RulesIcon() {
  return (
    <Icon>
      <rect x="3" y="8" width="18" height="8" rx="1" />
      <path d="M7 8v3M11 8v4M15 8v3M19 8v4" />
    </Icon>
  );
}

/** Pads joined by thin lines: the ratsnest. */
export function RatsnestIcon() {
  return (
    <Icon>
      <circle cx="5" cy="6" r="2" />
      <circle cx="19" cy="8" r="2" />
      <circle cx="9" cy="18" r="2" />
      <path d="M7 6.3 17 7.7M17.6 9.4 10.4 16.6" />
    </Icon>
  );
}
