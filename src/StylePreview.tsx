import { translateError } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { Check, ImagePlus } from "lucide-react";
import Generator, { freshDraft } from "./Generator";
import { AssetImage, ErrorBox, Modal } from "./components";
import { rpc, message, money } from "./api";
import { de, statuses, costStatuses } from "./i18n";
import type { AppState, Asset, Draft, Job, Project, RecordData } from "./types";

export type PreparedStylePreview = {
  project: Project;
  style: RecordData;
  description: string;
  blocks: Record<string, string>;
  aspect_ratio: string;
};
const PAGE_SIZE = 24;
const active = (job: Job) => ["queued", "running", "cancelling"].includes(job.status);

export default function StylePreview({ prepared, state, onClose, onSaved, onMode }: {
  prepared: PreparedStylePreview;
  state: AppState;
  onClose: () => void;
  onSaved: () => Promise<void>;
  onMode: (mode: string) => void;
}) {
  const [draft, setDraft] = useState<Draft>({
    ...freshDraft(prepared.project),
    description: prepared.description,
    style_id: prepared.style.id,
    blocks: prepared.blocks,
    aspect_ratio: prepared.aspect_ratio,
  });
  const [assets, setAssets] = useState<Asset[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [costs, setCosts] = useState<RecordData>({});
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [attempt, setAttempt] = useState(0);
  const submittedJobs = useRef<string[]>([]);
  const notifiedSignature = useRef("");
  const historyIds = useRef<Set<string> | null>(null);
  const { project, style } = prepared;
  useEffect(() => {
    let live = true;
    let fetching = false;
    const load = async () => {
      if (fetching) return;
      fetching = true;
      try {
        const [gallery, result, amounts] = await Promise.all([
          rpc("asset.list", { project_id: project.id, style_id: style.id, limit: PAGE_SIZE, offset }),
          rpc<Job[]>("job.list", { project_id: project.id }),
          rpc("cost.project", { project_id: project.id }),
        ]);
        if (!live) return;
        setAssets(gallery.items);
        setTotal(gallery.total);
        const styleJobs = result.filter((job) => job.snapshot?.style?.id === style.id);
        // Only jobs already finished on opening belong to the collapsed history.
        // An existing running job remains visible when it finishes or fails.
        if (!historyIds.current) historyIds.current = new Set(styleJobs
          .filter((job) => !active(job) && !submittedJobs.current.includes(job.id))
          .map((job) => job.id));
        setJobs(styleJobs);
        setCosts(amounts);
        const signature = gallery.items.map((a:Asset)=>a.id).join("|");
        if(signature !== notifiedSignature.current) { notifiedSignature.current=signature; await onSaved(); }
        const newAsset = result.find((job) => submittedJobs.current.includes(job.id) && job.asset_ids?.length)?.asset_ids[0];
        if (newAsset && gallery.items.some((asset: Asset) => asset.id === newAsset)) {
          setSelected(newAsset);
          submittedJobs.current = [];
        } else {
          setSelected((id) => gallery.items.some((asset: Asset) => asset.id === id) ? id : gallery.items[0]?.id || "");
        }
        setLoadError("");
      } catch (cause) {
        if (live) setLoadError(message(cause));
      } finally {
        fetching = false;
      }
    };
    void load();
    const timer = setInterval(() => void load(), 2000);
    return () => { live = false; clearInterval(timer); };
  }, [project.id, style.id, offset, refresh]);
  const selectedAsset = assets.find((asset) => asset.id === selected);
  async function submitted(result?: RecordData) {
    submittedJobs.current = (result?.jobs || []).map((job: Job) => job.id);
    setOffset(0);
    setRefresh((value) => value + 1);
  }
  async function adopt() {
    if (!selectedAsset || busy) return;
    setBusy(true);
    setError("");
    try {
      await rpc("style.preview.set", { id: style.id, asset_id: selectedAsset.id });
      await onSaved();
      onClose();
    } catch (cause) {
      setError(message(cause));
    } finally {
      setBusy(false);
    }
  }
  async function cancel(job: Job) {
    try {
      await rpc("job.cancel", { id: job.id });
      setRefresh((value) => value + 1);
    } catch (cause) {
      setError(message(cause));
    }
  }
  const currentJobs = jobs.filter((job) => !historyIds.current?.has(job.id));
  const historicalJobs = jobs.filter((job) => historyIds.current?.has(job.id));
  const renderJob = (job: Job) => <div key={job.id} className="style-preview-job"><span>{statuses[job.status] || job.status}</span><small>{job.cost_status === "unknown" ? `${de.costUnknownLabel} · ${de.estimated}: ${money(job.estimated_microusd)}` : `${money(job.cost_status === "estimated" ? job.estimated_microusd : job.actual_microusd)} · ${costStatuses[job.cost_status] || de.unknown}`}</small>{active(job) && <button className="button small" onClick={() => void cancel(job)}>{de.cancel}</button>}{job.error && <p>{translateError(job.error)}</p>}</div>;
  return (
    <Modal title={`${de.stylePreviewTitle} · ${style.name}`} onClose={onClose} wide className="style-preview-modal">
      <p className="style-preview-intro">{de.stylePreviewBody}</p>
      <div className="style-preview-workshop">
        <section className="style-preview-results" aria-label={de.stylePreviewCandidates}>
          <div className="style-preview-costs">
            <span>{de.stylePreviewCosts}</span>
            <strong>{money(costs.calculated_microusd)}</strong>
            <small>{money(costs.reserved_microusd)} {de.reserved.toLowerCase()}{costs.unknown_count ? ` · ${costs.unknown_count} × ${de.unknown}` : ""}</small>
          </div>
          <div className="style-preview-image">
            {selectedAsset ? <AssetImage id={selectedAsset.id} thumbnail={false} alt={`${de.stylePreviewTitle}: ${style.name}`} /> : <div className="style-preview-placeholder"><ImagePlus size={38} strokeWidth={1} /><p>{de.stylePreviewNoResults}</p></div>}
          </div>
          {selectedAsset && <>
            <p className="style-preview-caption">
              {de.stylePreviewVersion} {selectedAsset.style_version || "—"}
              {selectedAsset.provider === "test" && <strong>{de.stylePreviewTest}</strong>}
              {selectedAsset.style_version !== style.version && <strong>{de.stylePreviewStale}</strong>}
            </p>
            <button className="button primary" disabled={busy} onClick={() => void adopt()}><Check size={16} />{de.stylePreviewAdopt}</button>
          </>}
          <p className="hint">{de.stylePreviewGalleryHint}</p>
          <ErrorBox error={error || loadError} />
          {!!assets.length && <div className="style-preview-candidates">
            {assets.map((asset, index) => <button key={asset.id} className={selected === asset.id ? "selected" : ""} aria-label={`${de.preview} ${offset + index + 1}`} aria-pressed={selected === asset.id} onClick={() => setSelected(asset.id)}><AssetImage id={asset.id} alt={`${de.preview} ${offset + index + 1}`} /></button>)}
          </div>}
          {total > PAGE_SIZE && <div className="button-row"><button className="button small" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>{de.previous}</button><span className="hint">{offset + 1}–{Math.min(offset + PAGE_SIZE, total)} / {total}</span><button className="button small" disabled={offset + PAGE_SIZE >= total} onClick={() => setOffset(offset + PAGE_SIZE)}>{de.next}</button></div>}
          {!!currentJobs.length && <div className="style-preview-jobs" aria-label={de.jobs}>
            <h3>{de.jobs}</h3>
            {currentJobs.map(renderJob)}
          </div>}
          {!!historicalJobs.length && <details className="style-preview-history">
            <summary>{de.stylePreviewHistory} ({historicalJobs.length})</summary>
            {historicalJobs.map(renderJob)}
          </details>}
          <p className="hint">{de.stylePreviewJobNotice}</p>
        </section>
        <div className="style-preview-generator">
          <Generator key={attempt} state={state} project={project} assets={assets} draft={draft} setDraft={setDraft} onSubmitted={submitted} onMode={onMode} previewMode />
          {currentJobs.some((job) => job.status === "unknown") && <p className="warning">{de.stylePreviewUncertain}</p>}
          {(assets.length > 0 || currentJobs.some((job) => !active(job))) && <button className="button ghost" disabled={jobs.some(active)} onClick={() => setAttempt((value) => value + 1)}>{de.stylePreviewAnother}</button>}
        </div>
      </div>
    </Modal>
  );
}
