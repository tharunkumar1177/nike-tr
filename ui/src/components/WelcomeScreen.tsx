import type { AppInfo } from "../bindings/AppInfo";
import type { RecentProject } from "../bindings/RecentProject";
import type { ErrorReport } from "../ipc";

export type RecentState =
  | { status: "loading" }
  | { status: "ready"; projects: RecentProject[] }
  | { status: "error"; error: ErrorReport };

interface WelcomeScreenProps {
  appInfo: AppInfo | null;
  recent: RecentState;
  disabled: boolean;
  onNewProject: () => void;
  onOpenProject: () => void;
  onOpenRecent: (project: RecentProject) => void;
  onRetryRecent: () => void;
}

export function WelcomeScreen({
  appInfo,
  recent,
  disabled,
  onNewProject,
  onOpenProject,
  onOpenRecent,
  onRetryRecent,
}: WelcomeScreenProps) {
  return (
    <main className="welcome">
      <div className="welcome__panel">
        <header className="welcome__header">
          <h1 className="welcome__title">Tron</h1>
          <p className="welcome__subtitle">
            Schematic capture, circuit simulation, and PCB layout
          </p>
        </header>
        <div className="welcome__columns">
          <section
            className="welcome__section"
            aria-labelledby="start-heading"
          >
            <h2 id="start-heading" className="welcome__heading">
              Start
            </h2>
            <div className="welcome__actions">
              <button
                type="button"
                className="button button--primary"
                disabled={disabled}
                onClick={onNewProject}
              >
                New Project
              </button>
              <button
                type="button"
                className="button"
                disabled={disabled}
                onClick={onOpenProject}
              >
                Open Project...
              </button>
            </div>
          </section>
          <section
            className="welcome__section"
            aria-labelledby="recent-heading"
          >
            <h2 id="recent-heading" className="welcome__heading">
              Recent Projects
            </h2>
            <RecentList
              recent={recent}
              disabled={disabled}
              onOpenRecent={onOpenRecent}
              onRetryRecent={onRetryRecent}
            />
          </section>
        </div>
        {appInfo && (
          <p className="welcome__version">
            {appInfo.name} {appInfo.version}
          </p>
        )}
      </div>
    </main>
  );
}

type RecentListProps = Pick<
  WelcomeScreenProps,
  "recent" | "disabled" | "onOpenRecent" | "onRetryRecent"
>;

function RecentList({
  recent,
  disabled,
  onOpenRecent,
  onRetryRecent,
}: RecentListProps) {
  if (recent.status === "loading") {
    return <p className="welcome__empty">Loading recent projects...</p>;
  }
  if (recent.status === "error") {
    return (
      <div className="welcome__error" role="alert">
        <p>{recent.error.message}</p>
        {recent.error.detail && (
          <p className="welcome__error-detail">{recent.error.detail}</p>
        )}
        <button type="button" className="button" onClick={onRetryRecent}>
          Retry
        </button>
      </div>
    );
  }
  if (recent.projects.length === 0) {
    return <p className="welcome__empty">No recent projects.</p>;
  }
  return (
    <ul className="recent-list">
      {recent.projects.map((project) => (
        <li key={project.path}>
          <button
            type="button"
            className="recent-list__item"
            disabled={disabled}
            onClick={() => onOpenRecent(project)}
          >
            <span className="recent-list__name">
              {project.name}
              {!project.exists && (
                <span className="recent-list__missing">Missing</span>
              )}
            </span>
            <span className="recent-list__path">{project.path}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}
