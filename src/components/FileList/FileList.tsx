// Core
import type { ReactNode } from "react";
// Components
import Icon from "@/components/Icon/Icon";
// Hooks
import useI18n from "@/hooks/useI18n";
// Types
import type { FileId, FileInfo } from "@/services/fileforgeApi";
// Styles
import "./FileList.css";
// Utils
import formatBytes from "@/utils/formatBytes";

interface FileListProps {
  files: FileInfo[];
  onRemove: (id: FileId) => void;
  /** Tool-specific cells between the size and the remove button (status, result size, per-file actions). */
  renderDetails?: (file: FileInfo) => ReactNode;
  /** Disables removal while files are being processed. */
  locked?: boolean;
}

function FileList({ files, onRemove, renderDetails, locked = false }: FileListProps) {
  const { t, locale } = useI18n();

  return (
    <ul className="file-list">
      {files.map((file) => (
        <li key={file.id} className="file-list__row">
          <Icon name="document" className="file-list__icon" />
          <span className="file-list__name" title={file.name}>
            {file.name}
          </span>
          <span className="file-list__size">{formatBytes(file.size, locale)}</span>
          {renderDetails ? <div className="file-list__details">{renderDetails(file)}</div> : null}
          <button
            type="button"
            className="button button--ghost button--icon file-list__remove"
            onClick={() => onRemove(file.id)}
            disabled={locked}
            aria-label={t.intake.remove(file.name)}
          >
            <Icon name="close" />
          </button>
        </li>
      ))}
    </ul>
  );
}

export default FileList;
