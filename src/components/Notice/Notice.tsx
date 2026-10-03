// Components
import Icon from "@/components/Icon/Icon";
// Hooks
import useI18n from "@/hooks/useI18n";
// Styles
import "./Notice.css";

interface NoticeProps {
  /** One line each. */
  messages: string[];
  onDismiss: () => void;
}

/** Inline, dismissible message about the last action (skipped files, failed dialog). Never a modal. */
function Notice({ messages, onDismiss }: NoticeProps) {
  const { t } = useI18n();

  return (
    <div className="notice" role="status">
      <Icon name="warning" className="notice__icon" />
      <div className="notice__messages">
        {messages.map((message) => (
          <p key={message} className="notice__message">
            {message}
          </p>
        ))}
      </div>
      <button type="button" className="button button--ghost button--icon" onClick={onDismiss} aria-label={t.intake.dismiss}>
        <Icon name="close" />
      </button>
    </div>
  );
}

export default Notice;
