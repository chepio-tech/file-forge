// Components
import Icon from "@/components/Icon/Icon";
// Styles
import "./DropZone.css";
// Consts
import messages from "@/messages/messages";

interface DropZoneProps {
  formats: string;
  onChoose: () => void;
}

/**
 * Empty state of a tool: teaches both ways in (drop anywhere on the window, or the dialog). Drops are handled
 * window-wide by Rust, so this area is a target in looks only.
 */
function DropZone({ formats, onChoose }: DropZoneProps) {
  return (
    <div className="drop-zone">
      <div className="drop-zone__sheet" aria-hidden="true">
        <Icon name="tray" size={28} />
      </div>
      <p className="drop-zone__title">{messages.intake.dropTitle(formats)}</p>
      <p className="drop-zone__or">{messages.intake.or}</p>
      <button type="button" className="button button--primary" onClick={onChoose}>
        {messages.intake.choose}
      </button>
      <p className="drop-zone__privacy">{messages.intake.privacy}</p>
    </div>
  );
}

export default DropZone;
