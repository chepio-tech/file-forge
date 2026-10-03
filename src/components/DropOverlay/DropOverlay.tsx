// Components
import Icon from "@/components/Icon/Icon";
// Hooks
import useI18n from "@/hooks/useI18n";
// Styles
import "./DropOverlay.css";

interface DropOverlayProps {
  visible: boolean;
}

/** Covers the tool body while files are dragged over the window. Always rendered so it can fade both ways. */
function DropOverlay({ visible }: DropOverlayProps) {
  const { t } = useI18n();

  return (
    <div className={`drop-overlay${visible ? " drop-overlay--visible" : ""}`} aria-hidden={!visible}>
      <div className="drop-overlay__label">
        <Icon name="tray" size={20} />
        {t.intake.releaseToAdd}
      </div>
    </div>
  );
}

export default DropOverlay;
