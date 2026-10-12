import { Modal } from "./Modal";

interface UnsavedChangesDialogProps {
  projectName: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export function UnsavedChangesDialog({
  projectName,
  onSave,
  onDiscard,
  onCancel,
}: UnsavedChangesDialogProps) {
  return (
    <Modal title={`Save changes to ${projectName}?`} onDismiss={onCancel}>
      <p className="modal__message">
        Your changes will be lost if you don&apos;t save them.
      </p>
      <div className="modal__actions">
        <button
          type="button"
          className="button button--primary"
          data-autofocus
          onClick={onSave}
        >
          Save
        </button>
        <button type="button" className="button" onClick={onDiscard}>
          Don&apos;t Save
        </button>
        <button type="button" className="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </Modal>
  );
}
