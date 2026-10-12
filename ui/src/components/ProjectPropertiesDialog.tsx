import { type FormEvent, useId, useState } from "react";
import type { ProjectProperties } from "../bindings/ProjectProperties";
import type { ProjectSnapshot } from "../bindings/ProjectSnapshot";
import type { TitleBlockFields } from "../bindings/TitleBlockFields";
import type { ErrorReport } from "../ipc";
import { formatMm } from "../viewport";
import { Modal } from "./Modal";

const MAX_FIELD_LENGTH = 256;
const VISIBLE_COMMENTS = 4;

type TextFieldKey = "title" | "revision" | "date" | "company";

const TEXT_FIELDS: { key: TextFieldKey; label: string }[] = [
  { key: "title", label: "Title" },
  { key: "revision", label: "Revision" },
  { key: "date", label: "Date" },
  { key: "company", label: "Company" },
];

const GRID_PRESETS: { nm: number; label: string }[] = [
  { nm: 500_000, label: "0.5 mm" },
  { nm: 635_000, label: "0.635 mm (25 mil)" },
  { nm: 1_000_000, label: "1 mm" },
  { nm: 1_270_000, label: "1.27 mm (50 mil)" },
  { nm: 2_540_000, label: "2.54 mm (100 mil)" },
];

function sameTitleBlock(a: TitleBlockFields, b: TitleBlockFields): boolean {
  return (
    a.title === b.title &&
    a.revision === b.revision &&
    a.date === b.date &&
    a.company === b.company &&
    a.comments.length === b.comments.length &&
    a.comments.every((comment, index) => comment === b.comments[index])
  );
}

interface ProjectPropertiesDialogProps {
  project: ProjectSnapshot;
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (properties: ProjectProperties) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

export function ProjectPropertiesDialog({
  project,
  onApply,
  onCancel,
}: ProjectPropertiesDialogProps) {
  const idPrefix = useId();
  const errorId = useId();
  const [titleBlock, setTitleBlock] = useState<TitleBlockFields>(() => ({
    ...project.titleBlock,
    comments: [...project.titleBlock.comments],
  }));
  const [gridSizeNm, setGridSizeNm] = useState(project.gridSizeNm);
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const changed =
    gridSizeNm !== project.gridSizeNm ||
    !sameTitleBlock(titleBlock, project.titleBlock);
  const gridOptions = GRID_PRESETS.some((preset) => preset.nm === gridSizeNm)
    ? GRID_PRESETS
    : [
        { nm: gridSizeNm, label: `${formatMm(gridSizeNm)} mm` },
        ...GRID_PRESETS,
      ];

  const visibleComments = titleBlock.comments.slice(0, VISIBLE_COMMENTS);

  const updateField = (key: TextFieldKey, value: string) => {
    setTitleBlock((current) => {
      const next = { ...current };
      next[key] = value;
      return next;
    });
    setError(null);
  };

  const updateComment = (index: number, value: string) => {
    setTitleBlock((current) => ({
      ...current,
      comments: current.comments.map((comment, i) =>
        i === index ? value : comment,
      ),
    }));
    setError(null);
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!changed || submitting) {
      return;
    }
    setSubmitting(true);
    const failure = await onApply({ titleBlock, gridSizeNm });
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  return (
    <Modal title="Project Properties" onDismiss={onCancel}>
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <fieldset className="form__section">
          <legend className="form__legend">Title block</legend>
          {TEXT_FIELDS.map(({ key, label }, index) => (
            <div className="field field--inline" key={key}>
              <label className="field__label" htmlFor={`${idPrefix}-${key}`}>
                {label}
              </label>
              <input
                id={`${idPrefix}-${key}`}
                className="input"
                value={titleBlock[key]}
                maxLength={MAX_FIELD_LENGTH}
                autoComplete="off"
                data-autofocus={index === 0 ? true : undefined}
                onChange={(event) => updateField(key, event.target.value)}
              />
            </div>
          ))}
          {visibleComments.map((comment, index) => (
            <div className="field field--inline" key={`comment-${index}`}>
              <label
                className="field__label"
                htmlFor={`${idPrefix}-comment-${index}`}
              >
                Comment {index + 1}
              </label>
              <input
                id={`${idPrefix}-comment-${index}`}
                className="input"
                value={comment}
                maxLength={MAX_FIELD_LENGTH}
                autoComplete="off"
                onChange={(event) => updateComment(index, event.target.value)}
              />
            </div>
          ))}
        </fieldset>
        <fieldset className="form__section">
          <legend className="form__legend">Schematic</legend>
          <div className="field field--inline">
            <label className="field__label" htmlFor={`${idPrefix}-grid`}>
              Grid size
            </label>
            <select
              id={`${idPrefix}-grid`}
              className="input"
              value={gridSizeNm}
              onChange={(event) => {
                setGridSizeNm(Number(event.target.value));
                setError(null);
              }}
            >
              {gridOptions.map((option) => (
                <option key={option.nm} value={option.nm}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
        </fieldset>
        {error && (
          <p id={errorId} className="form__error" role="alert">
            {error.message}
          </p>
        )}
        <div className="modal__actions">
          <button
            type="submit"
            className="button button--primary"
            disabled={!changed || submitting}
          >
            Apply
          </button>
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
