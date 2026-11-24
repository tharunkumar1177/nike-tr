/**
 * The Update Board from Schematic dialog: plans the update, lists every
 * change and problem for review, and applies the reviewed plan as one undo
 * step. A plan the document has moved past is re-planned, not applied.
 */
import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { BoardUpdateReview } from "../bindings/BoardUpdateReview";
import type { BoardUpdateSettings } from "../bindings/BoardUpdateSettings";
import type { FootprintTextChange } from "../bindings/FootprintTextChange";
import type { PadNetUpdate } from "../bindings/PadNetUpdate";
import type { ReplacedFootprint } from "../bindings/ReplacedFootprint";
import type { UpdatedFootprint } from "../bindings/UpdatedFootprint";
import {
  type ErrorReport,
  applyBoardUpdate,
  planBoardUpdate,
  toErrorReport,
} from "../ipc";
import { Modal } from "./Modal";

type DialogState =
  | { status: "planning" }
  | { status: "review"; review: BoardUpdateReview }
  | { status: "applying"; review: BoardUpdateReview }
  | { status: "libraryNotReady" }
  | { status: "planFailed"; error: ErrorReport }
  | { status: "applyFailed"; review: BoardUpdateReview; error: ErrorReport };

const DEFAULT_SETTINGS: BoardUpdateSettings = {
  deleteExtra: true,
  replaceFootprints: true,
};
const PLANNING = "Planning update...";
const CONFLICT_NOTICE =
  "The document changed after this review. The review was refreshed.";

/** `count` and `noun`, with the noun pluralized when needed. */
function countOf(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/** Whether applying `review` would change nothing and nothing needs attention. */
function isUpToDate(review: BoardUpdateReview): boolean {
  return !review.hasChanges && review.problems.length === 0;
}

/** The screen reader summary of a finished review. */
function reviewSummary(review: BoardUpdateReview): string {
  if (isUpToDate(review)) {
    return "Board is up to date.";
  }
  const parts = [
    countOf(review.additions.length, "addition"),
    countOf(review.removals.length, "removal"),
    countOf(review.replacements.length, "replacement"),
    countOf(review.textChanges.length, "text change"),
    countOf(review.netChanges.length, "pad net change"),
    countOf(review.problems.length, "problem"),
  ];
  return `Review ready: ${parts.join(", ")}`;
}

/** "R1 10k — Resistor_SMD:R_0603_1608Metric". */
function footprintRow(item: UpdatedFootprint): string {
  return `${item.reference} ${item.value} — ${item.libId}`;
}

/** "R2: Device:R → Resistor_SMD:R_0603_1608Metric". */
function replacementRow(item: ReplacedFootprint): string {
  return `${item.reference}: ${item.from} → ${item.to}`;
}

/** "R5 Value: 10k → 22k". */
function textChangeRow(item: FootprintTextChange): string {
  const field = item.field === "reference" ? "Reference" : "Value";
  return `${item.reference} ${field}: ${item.from} → ${item.to}`;
}

/** "R4 pad 1: GND → VCC", with the unconnected net read as "no net". */
function netChangeRow(item: PadNetUpdate): string {
  const from = item.from ?? "no net";
  const to = item.to ?? "no net";
  return `${item.reference} pad ${item.pad}: ${from} → ${to}`;
}

interface SectionProps {
  title: string;
  rows: string[];
  warning?: boolean;
}

/** One titled list of review rows, left out when it has none. */
function Section({ title, rows, warning = false }: SectionProps) {
  if (rows.length === 0) {
    return null;
  }
  const heading = `${title} (${rows.length})`;
  const headingClass = warning
    ? "board-update__heading board-update__heading--warning"
    : "board-update__heading";
  return (
    <section className="board-update__section" aria-label={heading}>
      <h3 className={headingClass}>{heading}</h3>
      <ul className="board-update__rows">
        {rows.map((row, index) => (
          <li key={`${index}-${row}`} className="board-update__row">
            {row}
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Every change and problem in `review`, grouped into titled lists. */
function ReviewSections({ review }: { review: BoardUpdateReview }) {
  const problems = review.problems.map((problem) => problem.message);
  return (
    <div className="board-update__review">
      <Section title="Problems" warning rows={problems} />
      <Section title="Additions" rows={review.additions.map(footprintRow)} />
      <Section title="Removals" rows={review.removals.map(footprintRow)} />
      <Section
        title="Replacements"
        rows={review.replacements.map(replacementRow)}
      />
      <Section
        title="Text changes"
        rows={review.textChanges.map(textChangeRow)}
      />
      <Section
        title="Pad net changes"
        rows={review.netChanges.map(netChangeRow)}
      />
    </div>
  );
}

/** An error message with its optional detail, announced as an alert. */
function ErrorMessage({ error }: { error: ErrorReport }) {
  return (
    <div role="alert">
      <p className="form__error">{error.message}</p>
      {error.detail && <p className="modal__detail">{error.detail}</p>}
    </div>
  );
}

interface BoardUpdateDialogProps {
  /** Called once the update is applied; the owner then closes the dialog. */
  onApplied: () => void;
  /** Opens the component library download. */
  onDownloadLibrary: () => void;
  onClose: () => void;
}

/** Plans, reviews, and applies an update of the board from the schematic. */
export function BoardUpdateDialog({
  onApplied,
  onDownloadLibrary,
  onClose,
}: BoardUpdateDialogProps) {
  const [settings, setSettings] = useState(DEFAULT_SETTINGS);
  const [state, setState] = useState<DialogState>({ status: "planning" });
  const [notice, setNotice] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState(PLANNING);
  const ticketRef = useRef(0);

  /** Plans with `next`; replies to an older request are dropped. */
  const plan = useCallback((next: BoardUpdateSettings) => {
    const ticket = ++ticketRef.current;
    setState({ status: "planning" });
    setAnnouncement(PLANNING);
    planBoardUpdate(next).then(
      (review) => {
        if (ticket === ticketRef.current) {
          setState({ status: "review", review });
          setAnnouncement(reviewSummary(review));
        }
      },
      (failure: unknown) => {
        if (ticket !== ticketRef.current) {
          return;
        }
        const error = toErrorReport(failure);
        setState(
          error.kind === "libraryNotReady"
            ? { status: "libraryNotReady" }
            : { status: "planFailed", error },
        );
        setAnnouncement(error.message);
      },
    );
  }, []);

  useEffect(() => {
    plan(DEFAULT_SETTINGS);
    return () => {
      ticketRef.current += 1;
    };
  }, [plan]);

  /** Changes one option and plans again with it. */
  const toggle = (key: keyof BoardUpdateSettings, value: boolean) => {
    const next = { ...settings, [key]: value };
    setSettings(next);
    setNotice(null);
    plan(next);
  };

  /** Applies `review`; a conflict re-plans and says why. */
  const apply = (review: BoardUpdateReview) => {
    const ticket = ++ticketRef.current;
    setState({ status: "applying", review });
    setNotice(null);
    applyBoardUpdate(review.revision, settings).then(
      () => {
        if (ticket === ticketRef.current) {
          onApplied();
        }
      },
      (failure: unknown) => {
        if (ticket !== ticketRef.current) {
          return;
        }
        const error = toErrorReport(failure);
        if (error.kind === "boardUpdateConflict") {
          plan(settings);
          setNotice(CONFLICT_NOTICE);
          return;
        }
        setState(
          error.kind === "libraryNotReady"
            ? { status: "libraryNotReady" }
            : { status: "applyFailed", review, error },
        );
        setAnnouncement(error.message);
      },
    );
  };

  const closeButton = (
    <button type="button" className="button" onClick={onClose}>
      Close
    </button>
  );
  const disabledApply = (
    <button type="button" className="button button--primary" disabled>
      Apply Changes
    </button>
  );

  let content: ReactNode;
  switch (state.status) {
    case "planning":
      content = (
        <>
          <p className="modal__message" role="status">
            {PLANNING}
          </p>
          <div className="modal__actions">
            {closeButton}
            {disabledApply}
          </div>
        </>
      );
      break;
    case "libraryNotReady":
      content = (
        <>
          <p className="modal__message">
            The component library is needed to place footprints.
          </p>
          <div className="modal__actions">
            {closeButton}
            <button
              type="button"
              className="button button--primary"
              onClick={() => {
                onClose();
                onDownloadLibrary();
              }}
            >
              Download Library
            </button>
          </div>
        </>
      );
      break;
    case "planFailed":
      content = (
        <>
          <ErrorMessage error={state.error} />
          <div className="modal__actions">
            {closeButton}
            <button
              type="button"
              className="button button--primary"
              onClick={() => plan(settings)}
            >
              Retry
            </button>
          </div>
        </>
      );
      break;
    case "review":
    case "applying":
    case "applyFailed": {
      const { review } = state;
      content = (
        <>
          {isUpToDate(review) ? (
            <p className="modal__message">
              Board is up to date. No changes to apply.
            </p>
          ) : (
            <ReviewSections review={review} />
          )}
          {state.status === "applyFailed" && (
            <ErrorMessage error={state.error} />
          )}
          <div className="modal__actions">
            {closeButton}
            {review.hasChanges && (
              <button
                type="button"
                className="button button--primary"
                disabled={state.status === "applying"}
                onClick={() => apply(review)}
              >
                Apply Changes
              </button>
            )}
            {!review.hasChanges && !isUpToDate(review) && disabledApply}
          </div>
        </>
      );
      break;
    }
  }

  return (
    <Modal title="Update Board from Schematic" onDismiss={onClose}>
      {notice && (
        <p className="modal__message" role="status">
          {notice}
        </p>
      )}
      {state.status !== "libraryNotReady" && (
        <fieldset className="form__section">
          <legend className="form__legend">Options</legend>
          <label className="board-update__option">
            <input
              type="checkbox"
              checked={settings.deleteExtra}
              onChange={(event) =>
                toggle("deleteExtra", event.target.checked)
              }
            />
            Delete footprints with no symbols
          </label>
          <label className="board-update__option">
            <input
              type="checkbox"
              checked={settings.replaceFootprints}
              onChange={(event) =>
                toggle("replaceFootprints", event.target.checked)
              }
            />
            Replace footprints with those specified by symbols
          </label>
        </fieldset>
      )}
      {content}
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
    </Modal>
  );
}
