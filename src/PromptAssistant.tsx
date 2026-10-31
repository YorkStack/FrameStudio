import { formatText } from "./i18n";
import { t } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { Check, Sparkles } from "lucide-react";
import { rpc, message, money } from "./api";
import { ErrorBox, Field, Modal } from "./components";
import { de, statuses, costStatuses } from "./i18n";
import type { Job, RecordData } from "./types";

const active = (job?: Job) => Boolean(job && ["queued", "running", "cancelling"].includes(job.status));
export default function PromptAssistant({ projectId, text, target = "instruction", sections, pricingRevision, onClose, onApply }: {
  projectId: string;
  text: string;
  target?: "scene" | "character" | "instruction" | "all";
  sections?: Record<string,string>;
  pricingRevision?: string;
  onClose: () => void;
  onApply: (text: string, sections?: Record<string,string>) => void;
}) {
  const [source, setSource] = useState(text);
  const [sourceSections,setSourceSections] = useState(sections || {scene:"",character:"",instruction:text});
  const [edited,setEdited] = useState("");
  const [editedSections,setEditedSections] = useState<Record<string,string> | null>(null);
  const labels: Record<string,string> = {scene:de.promptSceneText,character:de.promptCharacterNotes,instruction:de.promptInstruction};
  const [provider, setProvider] = useState("anthropic");
  const [instruction, setInstruction] = useState("");
  const [estimate, setEstimate] = useState<RecordData | null>(null);
  const [checking, setChecking] = useState(true);
  const [busy, setBusy] = useState(false);
  const [job, setJob] = useState<Job>();
  const [error, setError] = useState("");
  const [pollError, setPollError] = useState("");
  const idempotency = useRef(crypto.randomUUID());
  const payload = { project_id: projectId, target, text:target === "all" ? JSON.stringify(sourceSections) : source, instruction, provider, model: provider === "test" ? "deterministic-prompt-v1" : "claude-sonnet-5-5" };
  const serialized = JSON.stringify(payload);
  useEffect(() => {
    let live = true;
    setChecking(true);
    setEstimate(null);
    idempotency.current = crypto.randomUUID();
    const timer = setTimeout(() => {
      rpc("assistant.estimate", JSON.parse(serialized)).then((result) => { if (live) { setEstimate(result); setError(""); } }).catch((cause) => { if (live) setError(message(cause)); }).finally(() => { if (live) setChecking(false); });
    }, 300);
    return () => { live = false; clearTimeout(timer); };
  }, [serialized, pricingRevision]);
  useEffect(() => {
    if (!active(job)) return;
    let live = true;
    let fetching = false;
    const load = async () => {
      if (fetching) return;
      fetching = true;
      try {
        const result = await rpc<Job>("job.get", { id: job!.id });
        if (live) { setJob(result); setPollError(""); }
      } catch (cause) {
        if (live) setPollError(message(cause));
      } finally { fetching = false; }
    };
    const timer = setInterval(() => void load(), 1000);
    void load();
    return () => { live = false; clearInterval(timer); };
  }, [job?.id, job?.status]);
  async function submit() {
    if (busy || job) return;
    setBusy(true);
    setError("");
    try {
      const result = await rpc("assistant.submit", { ...payload, idempotency_key: idempotency.current, expected_price_version: estimate?.price_version });
      setJob(result.jobs[0]);
    } catch (cause) { setError(message(cause)); }
    finally { setBusy(false); }
  }
  async function cancel() {
    if (!job) return;
    try { setJob(await rpc<Job>("job.cancel", { id: job.id })); }
    catch (cause) { setError(message(cause)); }
  }
  const estimateAmount = estimate?.per_request_microusd ?? estimate?.total_microusd;
  const result = edited;
  useEffect(()=>{ if(job?.status === "completed") {setEdited(job.result_text || "");setEditedSections(job.result_sections || null);} },[job?.id,job?.status]);
  const tooLong = target === "all" ? !editedSections || Object.values(editedSections).some(v=>new TextEncoder().encode(v).length > 8000) || !editedSections.instruction.trim() : new TextEncoder().encode(result).length > (target === "instruction" ? 12000 : 8000);

  return <Modal title={de.assistantTitle} onClose={onClose} wide>
    <p className="assistant-intro">{target === "all" ? de.promptAIAllHint : `${de.promptAITarget}: ${labels[target]}. ${de.promptAIOnlyTarget}`}</p>
    <div className="assistant-layout">
      <section>
        {target === "all" ? Object.entries(labels).map(([key,label])=><Field key={key} label={label}><textarea rows={3} disabled={busy||Boolean(job)} value={sourceSections[key]||""} onChange={e=>setSourceSections({...sourceSections,[key]:e.target.value})}/></Field>) : <Field label={de.assistantSentText} hint={de.promptAIOnlyTarget}><textarea className="assistant-source" value={source} disabled={busy||Boolean(job)} onChange={e=>setSource(e.target.value)} rows={8} /></Field>}
        <Field label={de.assistantInstruction}><textarea value={instruction} disabled={busy || Boolean(job)} maxLength={4000} rows={3} onChange={(event) => setInstruction(event.target.value)} placeholder={de.assistantInstructionExample} /></Field>
        <Field label={de.assistantProvider}><select value={provider} disabled={busy || Boolean(job)} onChange={(event) => setProvider(event.target.value)}><option value="anthropic">{de.claudeAssistant}</option><option value="test">{de.assistantTest}</option></select></Field>
        <p className="hint">{provider === "test" ? de.assistantTestNotice : de.assistantPrivacy}</p>
      </section>
      <section className="assistant-result">
        <h3>{de.assistantResult}</h3>
        {editedSections ? Object.entries(labels).map(([key,label])=><Field key={key} label={`${de.assistantResult}: ${label}`}><textarea rows={4} value={editedSections[key]||""} onChange={e=>setEditedSections({...editedSections,[key]:e.target.value})}/></Field>) : result ? <Field label={de.assistantResultText}><textarea value={result} onChange={e=>setEdited(e.target.value)} rows={12} /></Field> : <div className="assistant-empty"><Sparkles size={30} strokeWidth={1} /><p>{job ? statuses[job.status] || job.status : de.assistantNoResult}</p></div>}
        {job && <div className="assistant-job" aria-live="polite"><strong>{statuses[job.status] || job.status}</strong><span>{money(job.cost_status === "estimated" ? job.estimated_microusd : job.actual_microusd)} · {costStatuses[job.cost_status] || de.estimated}</span>{active(job) && <button className="button small" onClick={() => void cancel()}>{de.cancel}</button>}</div>}
        {job?.error && <ErrorBox error={job.error} />}
        {["unknown", "uncertain"].includes(job?.status || "") && <p className="warning">{de.assistantUncertain}</p>}
        
        {tooLong && result && <p className="warning">{de.assistantTooLong}</p>}
      </section>
    </div>
    <ErrorBox error={error || pollError} />
    <div className="assistant-footer">{result && job?.status === "completed" && <button className="button primary" disabled={tooLong} onClick={() => onApply(result, editedSections || undefined)}><Check size={16} />{target === "all" ? t("Drei Bereiche übernehmen") : target === "instruction" ? de.assistantApply : formatText("{0} übernehmen",labels[target])}</button>}<div><span>{de.assistantEstimate}</span><strong>{checking ? "—" : money(estimateAmount)}</strong><small>{de.assistantCostNotice}</small></div><button className="button primary" disabled={busy || Boolean(job) || checking || estimateAmount == null || (target !== "all" && !source.trim())} onClick={() => void submit()}><Sparkles size={16} />{busy ? de.busy : job ? de.requestSent : de.assistantRequest}</button></div>
    {estimate?.warnings?.map((warning: string, index: number) => <p className="hint" key={index}>{t(warning)}</p>)}
    <p className="hint">{de.assistantCloseNotice}</p>
  </Modal>;
}
