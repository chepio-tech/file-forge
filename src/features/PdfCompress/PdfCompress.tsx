// Components
import DropOverlay from "@/components/DropOverlay/DropOverlay";
import DropZone from "@/components/DropZone/DropZone";
import FileList from "@/components/FileList/FileList";
import Icon from "@/components/Icon/Icon";
import Notice from "@/components/Notice/Notice";
import ToolBody from "@/components/ToolBody/ToolBody";
import ToolHeader from "@/components/ToolHeader/ToolHeader";
// Hooks
import useDragHover from "@/hooks/useDragHover";
import useFileIntake from "@/hooks/useFileIntake";
import useI18n from "@/hooks/useI18n";
// Types
import type { ToolPanelProps } from "@/features/toolPanel";
// Styles
import "./PdfCompress.css";
// Utils
import formatBytes from "@/utils/formatBytes";

function PdfCompress({ tool, active }: ToolPanelProps) {
  const { t, locale } = useI18n();
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const { files } = intake;
  const totalSize = files.reduce((sum, file) => sum + file.size, 0);

  return (
    <>
      <ToolHeader
        title={t.tools.pdfCompress.title}
        description={t.tools.pdfCompress.description}
        actions={
          files.length > 0 ? (
            <>
              <button type="button" className="button button--ghost" onClick={intake.clear}>
                {t.intake.clear}
              </button>
              <button type="button" className="button" onClick={() => void intake.pick()}>
                <Icon name="plus" />
                {t.intake.addMore}
              </button>
            </>
          ) : null
        }
      />
      <ToolBody>
        {intake.notice.length > 0 ? <Notice messages={intake.notice} onDismiss={intake.dismissNotice} /> : null}
        {files.length === 0 ? (
          <DropZone formats={tool.formats} onChoose={() => void intake.pick()} />
        ) : (
          <section className="pdf-compress__files" aria-label={t.intake.filesHeading(files.length)}>
            <div className="pdf-compress__summary">
              <span>{t.intake.filesHeading(files.length)}</span>
              <span className="pdf-compress__total">{t.intake.totalSize(formatBytes(totalSize, locale))}</span>
            </div>
            <FileList files={files} onRemove={intake.remove} />
          </section>
        )}
        <DropOverlay visible={hovering} />
      </ToolBody>
    </>
  );
}

export default PdfCompress;
