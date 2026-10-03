// Components
import Icon from "@/components/Icon/Icon";
// Styles
import "./Notice.css";
// Consts
import messages from "@/messages/messages";

interface NoticeProps {
  /** One line each. */
  lines: string[];
  onDismiss: () => void;
}

/** Inline, dismissible message about the last action (skipped files, failed dialog). Never a modal. */
function Notice({ lines, onDismiss }: NoticeProps) {
  return (
    <div className="notice" role="status">
      <Icon name="warning" className="notice__icon" />
      <div className="notice__messages">
        {lines.map((line) => (
          <p key={line} className="notice__message">
            {line}
          </p>
        ))}
      </div>
      <button
        type="button"
        className="button button--ghost button--icon"
        onClick={onDismiss}
        aria-label={messages.intake.dismiss}
      >
        <Icon name="close" />
      </button>
    </div>
  );
}

export default Notice;
