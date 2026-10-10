// Core
import { useEffect, useRef, useState } from "react";
// Components
import DropZone from "@/components/DropZone/DropZone";
import DropOverlay from "@/components/DropOverlay/DropOverlay";
import FileList from "@/components/FileList/FileList";
import Notice from "@/components/Notice/Notice";
import ToolBody from "@/components/ToolBody/ToolBody";
import ToolHeader from "@/components/ToolHeader/ToolHeader";
// Hooks
import useFileIntake from "@/hooks/useFileIntake";
import useDragHover from "@/hooks/useDragHover";
import useCompressionJobs from "@/hooks/useCompressionJobs";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { BackgroundOptions, BackgroundReport, ModelStatus, DownloadProgress } from "@/services/fileforgeApi";
import type { CompressionEngine } from "@/hooks/useCompressionJobs";
import type { ToolPanelProps } from "@/features/toolPanel";
// Styles
import "./ImageBackground.css";
// Utils
import formatBytes from "@/utils/formatBytes";
import { errorMessage } from "@/utils/errorMessage";
// Consts
import messages from "@/messages/messages";

const engine: CompressionEngine<BackgroundOptions, BackgroundReport> = {
  compress: fileforgeApi.removeBackground,
  optionsKey: ({ format, background, crop, point }) => JSON.stringify({ format, background, crop, point }),
  removalRequested: () => true,
  keptOriginal: () => false,
  sizes: (report) => report,
};

function ImageBackground({ tool, active }: ToolPanelProps) {
  const text = messages.background;
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const [options, setOptions] = useState<BackgroundOptions>({ format: "png", background: null, crop: false, point: null });
  const jobs = useCompressionJobs(intake.files, engine);
  const [model, setModel] = useState<ModelStatus | null>(null);
  const [modelBusy, setModelBusy] = useState(false);
  const [modelError, setModelError] = useState<string | null>(null);
  const [modelCheck, setModelCheck] = useState(0);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [original, setOriginal] = useState<{ id: number; src: string } | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [previewError, setPreviewError] = useState<{ id: number; message: string } | null>(null);
  const [previewAttempt, setPreviewAttempt] = useState(0);
  const [subjectPoints, setSubjectPoints] = useState<ReadonlyMap<number, [number, number]>>(new Map());
  const [point, setPoint] = useState<[number, number]>([50, 50]);
  const operating = useRef(false);
  const busy = jobs.busy || modelBusy;
  const finished = [...jobs.jobs].filter(([, job]) => job.status === "done");
  const selectedId = selected !== null && intake.files.some((file) => file.id === selected) ? selected : intake.files[0]?.id;
  const selectedJob = selectedId === undefined ? undefined : jobs.jobs.get(selectedId);
  const report = selectedJob?.status === "done" ? selectedJob.report : null;
  const selectedFile = intake.files.find((file) => file.id === selectedId);
  const optionsForFile = (id: number): BackgroundOptions => ({ ...options, point: subjectPoints.get(id) ?? null });
  const outdated = finished.some(([id, job]) => job.status === "done" && job.optionsKey !== engine.optionsKey(optionsForFile(id)));
  const lines = [...intake.notice, ...(jobs.notice ? [jobs.notice] : []), ...(notice ? [notice] : [])];

  useEffect(() => {
    if (!active) { void fileforgeApi.releaseBackgroundModel().catch(() => {}); return; }
    let disposed = false;
    setModelError(null);
    setModel(null);
    fileforgeApi.backgroundModelStatus().then((status) => { if (!disposed) setModel(status); }).catch((error: unknown) => { if (!disposed) setModelError(errorMessage(error)); });
    return () => { disposed = true; };
  }, [active, modelCheck]);

  useEffect(() => {
    if (!active || selectedId === undefined) return;
    let disposed = false;
    setPreviewError(null);
    fileforgeApi.backgroundPreview(selectedId).then((src) => { if (!disposed) setOriginal({ id: selectedId, src }); }).catch((error: unknown) => { if (!disposed) setPreviewError({ id: selectedId, message: errorMessage(error) }); });
    return () => { disposed = true; };
  }, [active, selectedId, previewAttempt]);

  useEffect(() => {
    const savedPoint = selectedId === undefined ? undefined : subjectPoints.get(selectedId);
    setPoint(savedPoint ? [Math.round(savedPoint[0] * 100), Math.round(savedPoint[1] * 100)] : [50, 50]);
  }, [selectedId, subjectPoints]);

  useEffect(() => {
    setSubjectPoints((current) => {
      const kept = [...current].filter(([id]) => intake.files.some((file) => file.id === id));
      return kept.length === current.size ? current : new Map(kept);
    });
  }, [intake.files]);

  async function modelAction(remove: boolean) {
    if (operating.current || jobs.busy) return;
    operating.current = true;
    setModelBusy(true);
    setNotice(null);
    try {
      if (remove) { await fileforgeApi.removeBackgroundModel(); setModel((current) => current ? { ...current, installed: false } : current); }
      else {
        setDownload({ downloaded: 0, total: model?.downloadBytes ?? 0 });
        setModel(await fileforgeApi.downloadBackgroundModel(setDownload));
      }
    } catch (error) { setNotice(errorMessage(error)); }
    finally { operating.current = false; setModelBusy(false); setDownload(null); }
  }

  async function pickSubject(nextPoint: [number, number] | null) {
    if (selectedId === undefined || busy || !model?.installed) return;
    setSubjectPoints((current) => {
      const next = new Map(current);
      if (nextPoint) next.set(selectedId, nextPoint); else next.delete(selectedId);
      return next;
    });
    await jobs.compressOne(selectedId, { ...options, point: nextPoint });
  }

  return <>
    <ToolHeader title={messages.tools.imageBackground.title} description={messages.tools.imageBackground.description} actions={<>
      {intake.files.length > 0 && <button type="button" className="button button--ghost" disabled={busy} onClick={intake.clear}>{messages.intake.clear}</button>}
      <button type="button" className="button" disabled={busy} onClick={() => void intake.pick()}>{messages.intake.addMore}</button>
    </>} />
    <ToolBody>
      {lines.length > 0 && <Notice lines={lines} onDismiss={() => { setNotice(null); intake.dismissNotice(); jobs.dismissNotice(); }} />}
      <section className="background-panel__model" aria-label={text.model}>
        <div><strong>{text.model}</strong><p>{text.modelHint}</p><span role="status">{download ? text.downloading(formatBytes(download.downloaded), formatBytes(download.total)) : model?.installed ? text.ready : modelError ?? (model === null ? text.checking : null)}</span></div>
        {modelError ? <button type="button" className="button" disabled={busy} onClick={() => setModelCheck((attempt) => attempt + 1)}>{text.retryModel}</button>
          : download ? <button type="button" className="button" onClick={() => { void fileforgeApi.cancelCompression().catch((error: unknown) => setNotice(errorMessage(error))); }}>{messages.compression.cancel}</button>
          : model?.installed ? <button type="button" className="button button--ghost" disabled={busy} onClick={() => void modelAction(true)}>{text.removeModel}</button>
          : <button type="button" className="button" disabled={busy || model === null} onClick={() => void modelAction(false)}>{text.download(formatBytes(model?.downloadBytes ?? 0))}</button>}
      </section>
      <div className="background-panel__settings">
        <label>{text.output}<select disabled={busy} value={options.format} onChange={(e) => setOptions({ ...options, format: e.target.value as BackgroundOptions["format"] })}><option value="png">{messages.image.formats.png}</option><option value="webp">{messages.image.formats.webp}</option></select></label>
        <label>{text.background}<select disabled={busy} value={options.background ? "solid" : "transparent"} onChange={(e) => setOptions({ ...options, background: e.target.value === "solid" ? [255, 255, 255] : null })}><option value="transparent">{text.transparent}</option><option value="solid">{text.solid}</option></select></label>
        {options.background && <label>{text.color}<input type="color" disabled={busy} value={`#${options.background.map((c) => c.toString(16).padStart(2, "0")).join("")}`} onChange={(e) => setOptions({ ...options, background: [1, 3, 5].map((i) => parseInt(e.target.value.slice(i, i + 2), 16)) as [number, number, number] })} /></label>}
        <label className="background-panel__check"><input type="checkbox" disabled={busy} checked={options.crop} onChange={(e) => setOptions({ ...options, crop: e.target.checked })} />{text.crop}</label>
      </div>
      <p className="background-panel__hint">{text.limits}</p>
      {intake.files.length === 0 ? <DropZone formats={tool.formats} onChoose={() => void intake.pick()} /> : <div className="background-panel__files"><FileList files={intake.files} onRemove={intake.remove} locked={busy} renderDetails={(file) => {
        const job = jobs.jobs.get(file.id);
        if (job?.status === "done") return <span className="background-panel__file-actions">
          <span>{text.outputSize(formatBytes(job.report.outputSize))}</span>
          <button type="button" className="button button--ghost" disabled={busy} aria-label={text.view(file.name)} aria-pressed={selectedId === file.id} onClick={() => { setSelected(file.id); setPreviewAttempt((attempt) => attempt + 1); }}>{text.preview}</button>
          <button type="button" className="button" disabled={busy} onClick={() => void jobs.save(file.id)}>{jobs.saving.has(file.id) ? messages.compression.saving : messages.compression.save}</button>
          {job.savedName && <button type="button" className="button button--ghost" title={messages.compression.savedAs(job.savedName)} onClick={() => jobs.reveal(file.id)}>{messages.compression.saved}</button>}
        </span>;
        return <span className="background-panel__file-actions"><button type="button" className="button button--ghost" disabled={busy} aria-label={text.view(file.name)} onClick={() => { setSelected(file.id); setPreviewAttempt((attempt) => attempt + 1); }}>{text.preview}</button><span role="status">{job?.status === "working" ? text.working : job?.status === "error" ? job.message : job?.status === "cancelled" ? messages.compression.cancelled : null}</span></span>;
      }} /></div>}
      {selectedFile && previewError && previewError.id === selectedId && <section className="background-panel__preview-error" aria-label={text.view(selectedFile.name)}><p role="status">{previewError.message}</p><button type="button" className="button" disabled={busy} onClick={() => setPreviewAttempt((attempt) => attempt + 1)}>{text.retryPreview}</button></section>}
      {selectedFile && original && original.id === selectedId && <section className="background-panel__preview" aria-label={text.view(selectedFile.name)}>
        <div className="background-panel__preview-heading"><strong>{selectedFile.name}</strong><button type="button" className="button button--ghost" disabled={busy || !model?.installed} onClick={() => void pickSubject(null)}>{text.automatic}</button></div>
        <div className="background-panel__images">
          <figure><figcaption>{text.original}</figcaption><button type="button" className="background-panel__canvas" aria-label={text.pick} disabled={busy || !model?.installed} onClick={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            if (e.detail === 0) { void pickSubject([point[0] / 100, point[1] / 100]); return; }
            const p: [number, number] = [Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width)), Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height))];
            setPoint([Math.round(p[0] * 100), Math.round(p[1] * 100)]); void pickSubject(p);
          }}><img src={original.src} alt={selectedFile.name} /></button></figure>
          {report && <figure><figcaption>{text.result}</figcaption><div className="background-panel__canvas"><img src={report.preview} alt={text.result} /></div><p>{report.width}×{report.height} · {formatBytes(report.outputSize)}</p></figure>}
        </div>
        <p className="background-panel__hint">{text.keyboardHint}</p>
        <div className="background-panel__coordinates">
          {([0, 1] as const).map((axis) => <label key={axis}>{axis === 0 ? text.pointX : text.pointY}<input type="number" min={0} max={100} step={1} disabled={busy} value={point[axis]} onChange={(e) => { const value = Number(e.target.value); if (Number.isFinite(value)) setPoint(axis === 0 ? [Math.max(0, Math.min(100, value)), point[1]] : [point[0], Math.max(0, Math.min(100, value))]); }} /></label>)}
          <button type="button" className="button" disabled={busy || !model?.installed} onClick={() => void pickSubject([point[0] / 100, point[1] / 100])}>{text.applyPoint}</button>
        </div>
      </section>}
      <DropOverlay visible={hovering} />
    </ToolBody>
    {intake.files.length > 0 && <footer className="background-panel__actions">
      <span role="status">{outdated ? text.outdated : jobs.progress ? text.progress(jobs.progress.current, jobs.progress.total) : null}</span>
      {finished.length > 0 && <button type="button" className="button" disabled={busy} onClick={() => void jobs.saveAll()}>{messages.compression.saveAll(finished.length)}</button>}
      {jobs.running && <button type="button" className="button" disabled={jobs.cancelling} onClick={jobs.cancel}>{jobs.cancelling ? messages.compression.cancelling : messages.compression.cancel}</button>}
      <button type="button" className="button button--primary" disabled={busy || !model?.installed} onClick={() => void jobs.compressAll(options, optionsForFile)}>{text.run(intake.files.length)}</button>
    </footer>}
  </>;
}
export default ImageBackground;
