// Components
import Icon from "@/components/Icon/Icon";
// Hooks
import useI18n from "@/hooks/useI18n";
// Styles
import "./DropZone.css";

interface DropZoneProps {
  formats: string;
  onChoose: () => void;
}

/**
 * Empty state of a tool: teaches both ways in (drop anywhere on the window, or the dialog). Drops are handled
 * window-wide by Rust, so this area is a target in looks only.
 */
function DropZone({ formats, onChoose }: DropZoneProps) {
  const { t } = useI18n();

  return (
    <div className="drop-zone">
      <div className="drop-zone__sheet" aria-hidden="true">
        <Icon name="tray" size={28} />
      </div>
      <p className="drop-zone__title">{t.intake.dropTitle(formats)}</p>
      <p className="drop-zone__or">{t.intake.or}</p>
      <button type="button" className="button button--primary" onClick={onChoose}>
        {t.intake.choose}
      </button>
      <p className="drop-zone__privacy">{t.intake.privacy}</p>
    </div>
  );
}

export default DropZone;
