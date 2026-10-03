// Components
import Icon from "@/components/Icon/Icon";
// Styles
import "./DropOverlay.css";
// Consts
import messages from "@/messages/messages";

interface DropOverlayProps {
  visible: boolean;
}

/** Covers the tool body while files are dragged over the window. Always rendered so it can fade both ways. */
function DropOverlay({ visible }: DropOverlayProps) {
  return (
    <div className={`drop-overlay${visible ? " drop-overlay--visible" : ""}`} aria-hidden={!visible}>
      <div className="drop-overlay__label">
        <Icon name="tray" size={20} />
        {messages.intake.releaseToAdd}
      </div>
    </div>
  );
}

export default DropOverlay;
