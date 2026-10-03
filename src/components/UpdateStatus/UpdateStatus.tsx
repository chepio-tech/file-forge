// Components
import Spinner from "@/components/Spinner/Spinner";
// Hooks
import { useUpdates } from "./useUpdates";
// Styles
import "./UpdateStatus.css";
// Consts
import messages from "@/messages/messages";

/** App updates at the bottom of the sidebar: a quiet check button until a new version is found (ADR-0013). */
function UpdateStatus() {
  const { state, check, install, keep } = useUpdates();

  return (
    <div className="update-status" aria-live="polite">
      {state.status === "idle" && (
        <button type="button" className="button button--ghost update-status__check" onClick={check}>
          {messages.updates.check}
        </button>
      )}

      {state.status === "checking" && (
        <p className="update-status__line">
          <Spinner />
          {messages.updates.checking}
        </p>
      )}

      {state.status === "current" && <p className="update-status__line">{messages.updates.upToDate(state.version)}</p>}

      {state.status === "checkFailed" && (
        <>
          <p className="update-status__line">{messages.updates.checkFailed}</p>
          <div className="update-status__actions">
            <button type="button" className="button" onClick={check}>
              {messages.updates.retry}
            </button>
          </div>
        </>
      )}

      {state.status === "available" && (
        <>
          <p className="update-status__line update-status__line--news">{messages.updates.available(state.version)}</p>
          {state.message && <p className="update-status__line update-status__line--error">{state.message}</p>}
          <div className="update-status__actions">
            <button type="button" className="button button--primary" onClick={() => install()}>
              {messages.updates.install}
            </button>
          </div>
        </>
      )}

      {state.status === "confirm" && (
        <>
          <p className="update-status__line update-status__line--warning">{messages.updates.unsaved}</p>
          <div className="update-status__actions">
            <button type="button" className="button button--primary" onClick={() => install(true)}>
              {messages.updates.discard}
            </button>
            <button type="button" className="button button--ghost" onClick={keep}>
              {messages.updates.keep}
            </button>
          </div>
        </>
      )}

      {state.status === "installing" && (
        <p className="update-status__line">
          <Spinner />
          {messages.updates.installing}
        </p>
      )}
    </div>
  );
}

export default UpdateStatus;
