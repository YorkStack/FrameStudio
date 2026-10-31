import { LoaderCircle, CircleCheck, Info } from "lucide-react";
import { de } from "./i18n";
import type { Job } from "./types";

const groupKey = (job: Job) => job.series_id ? `series:${job.series_id}` : job.idempotency_key ? `batch:${String(job.idempotency_key).replace(/:\d+$/, "")}` : job.id;
const active = (job: Job) => ["queued", "running", "cancelling"].includes(job.status);
export default function GenerationProgress({ jobs, onDetails, onExportSeries }: { jobs: Job[]; onDetails: () => void; onExportSeries: (id: string) => void }) {
  const images = jobs.filter(j => j.kind !== "prompt_assist");
  const ordered = [...images].sort((a,b) => String(b.created_at || "").localeCompare(String(a.created_at || "")));
  const anchor = ordered.find(active) || ordered[0];
  if (!anchor) return null;
  const group = images.filter(j => groupKey(j) === groupKey(anchor));
  const done = group.filter(j => j.status === "completed").length;
  const waiting = group.filter(j => j.status === "queued").length;
  const working = group.filter(j => ["running", "cancelling"].includes(j.status));
  const stopped = group.filter(j => !active(j) && j.status !== "completed").length;
  const busy = group.some(active);
  const current = working[0];
  const title = busy ? de.generationWorking : stopped ? de.generationStopped : de.generationComplete;
  const detail = current?.description || current?.snapshot?.parameters?.description;
  return <section className="generation-progress" aria-label={de.generationProgress}>
    <div className="generation-progress-heading" role="status" aria-live="polite">
      {busy ? <LoaderCircle className="generation-spinner" size={20} /> : stopped ? <Info size={20} /> : <CircleCheck size={20} />}
      <strong>{title}</strong><span>{done} / {group.length} {de.generationImagesDone}</span>
      {anchor.series_id && done > 0 && <button className="button small" onClick={() => onExportSeries(anchor.series_id)}>{de.exportSeries}</button>}
      <button className="text-button" onClick={onDetails}>{de.generationDetails}</button>
    </div>
    <progress aria-label={de.generationImagesDone} max={group.length} value={group.length === 1 && busy ? undefined : done} />
    <div className="generation-progress-detail">
      <span>{working.length} {de.generationActive} · {waiting} {de.generationWaiting}{stopped > 0 ? ` · ${stopped} ${de.generationUnfinished}` : ""}</span>
      {detail && <span className="generation-current">{de.generationCurrent}: {detail}</span>}
      {busy && <small>{de.generationNoPercent}</small>}
    </div>
  </section>;
}
