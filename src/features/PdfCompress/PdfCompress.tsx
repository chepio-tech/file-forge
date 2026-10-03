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
// Types
import type { ToolPanelProps } from "@/features/toolPanel";
// Styles
import "./PdfCompress.css";
// Utils
import formatBytes from "@/utils/formatBytes";
// Consts
import messages from "@/messages/messages";

function PdfCompress({ tool, active }: ToolPanelProps) {
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const { files } = intake;
  const totalSize = files.reduce((sum, file) => sum + file.size, 0);

  return (
    <>
      <ToolHeader
        title={messages.tools.pdfCompress.title}
        description={messages.tools.pdfCompress.description}
        actions={
          files.length > 0 ? (
            <>
              <button type="button" className="button button--ghost" onClick={intake.clear}>
                {messages.intake.clear}
              </button>
              <button type="button" className="button" onClick={() => void intake.pick()}>
                <Icon name="plus" />
                {messages.intake.addMore}
              </button>
            </>
          ) : null
        }
      />
      <ToolBody>
        {intake.notice.length > 0 ? <Notice lines={intake.notice} onDismiss={intake.dismissNotice} /> : null}
        {files.length === 0 ? (
          <DropZone formats={tool.formats} onChoose={() => void intake.pick()} />
        ) : (
          <section className="pdf-compress__files" aria-label={messages.intake.filesHeading(files.length)}>
            <div className="pdf-compress__summary">
              <span>{messages.intake.filesHeading(files.length)}</span>
              <span className="pdf-compress__total">{messages.intake.totalSize(formatBytes(totalSize))}</span>
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
