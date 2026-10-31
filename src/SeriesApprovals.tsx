import { translateError } from "./i18n";
import { locale } from "./i18n";
import { t } from "./i18n";
import { useCallback, useEffect, useRef, useState } from "react";
import { ListChecks } from "lucide-react";
import { rpc, money, message } from "./api";
import { ErrorBox, Modal } from "./components";
import { de, statuses } from "./i18n";
import type { AppState, RecordData } from "./types";

export default function SeriesApprovals({ state, onOpenProject, onExportSeries }: { state: AppState; onExportSeries: (series: RecordData) => Promise<void>; onOpenProject: (series: RecordData) => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [series, setSeries] = useState<RecordData[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const [actionError, setActionError] = useState<{id: string; text: string} | null>(null);
  const actionPending = useRef(false);
  const refresh = useCallback(async () => {
    try { setSeries(await rpc("series.list")); }
    catch (e) { setError(message(e)); }
  }, []);
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [refresh]);
  const action = async (op: string, id: string, args: RecordData = {}) => {
    if (actionPending.current) return;
    actionPending.current = true;
    setBusy(id); setError(""); setActionError(null);
    let approved = false;
    try {
      if (op === "series.approve") {
        const selected = args.selection?.provider;
        const providers = selected ? [selected] : [...new Set(series.find(s => s.id === id)?.items.map((item: RecordData) => item.snapshot.parameters.provider) || [])];
        for (const provider of providers) if (provider !== "test") {
          try { await rpc("secret.check", { provider }); }
          catch (e) {
            const name = ({gemini:"Google Gemini", openai:"OpenAI", anthropic:"Anthropic / Claude"} as Record<string,string>)[provider] || provider;
            throw new Error(`${de.seriesKeyCheckFailed} ${name}. ${message(e)}\n${de.seriesKeyCheckHelp} ${name}.`);
          }
        }
      }
      const result = await rpc<RecordData>(op === "series.export" ? "series.get" : op, { id, ...args });
      if (op === "series.export") { await onExportSeries(result); setOpen(false); return; }
      approved = op === "series.approve";
      setSeries(current => current.map(item => item.id === id ? result : item));
      if (approved || op === "series.get") {
        await onOpenProject(result);
        setOpen(false);
      }
      await refresh();
    } catch (e) {
      setActionError({id, text: `${approved ? `${de.seriesOpenFailed} ` : ""}${message(e)}`});
    } finally {
      actionPending.current = false;
      setBusy("");
    }
  };
  const pending = series.filter(s => s.status === "pending").length;
  const names: Record<string, string> = { pending: de.seriesPending, approved: de.seriesApproved, rejected: de.seriesRejected, cancelled: de.seriesCancelled };
  return <>
    <button className={`button small ${pending ? "primary" : ""}`} onClick={() => { setError(""); setActionError(null); setOpen(true); }} aria-label={`${de.seriesRequests}: ${pending}`}>
      <ListChecks size={16} /><span>{de.seriesRequests}{pending ? ` · ${pending}` : ""}{error && !open ? " !" : ""}</span>
    </button>
    {open && <Modal title={de.seriesRequests} onClose={() => { if (!actionPending.current) setOpen(false); }} wide>
      <div className="series-approvals">
        <p className="muted">{de.seriesApprovalIntro}</p>
        <ErrorBox error={error} />
        {!series.length && <p className="series-empty">{de.seriesEmpty}</p>}
        {series.map(s => <SeriesCard key={s.id} s={s} state={state} busy={busy} action={action} names={names} actionError={actionError && actionError.id === s.id ? actionError.text : ""} />)}
      </div>
    </Modal>}
  </>;
}

type Option = { label: string; selection?: RecordData; providers: string[]; quote?: RecordData; error?: string };
function SeriesCard({ s, state, busy, action, names, actionError }: { actionError: string; s: RecordData; state: AppState; busy: string; action: (op: string, id: string, args?: RecordData) => Promise<void>; names: Record<string, string> }) {
  const feedback = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (actionError) {
      feedback.current?.scrollIntoView({block:"center", behavior:"instant"});
      feedback.current?.focus({preventScroll:true});
    }
  }, [actionError]);
  const [options, setOptions] = useState<Option[]>([]);
  const [choice, setChoice] = useState(0);
  const [loading, setLoading] = useState(true);
  const [quoteError, setQuoteError] = useState("");
  const [extendLimit, setExtendLimit] = useState(false);
  // Polling the series list must not discard the user's reviewed selection.
  const keySignature = JSON.stringify(state.key_status?.providers || {});
  useEffect(() => {
    if (s.status !== "pending") return;
    let live = true;
    setLoading(true); setQuoteError(""); setExtendLimit(false); setChoice(0);
    const hasPaid = ["gemini", "openai"].some(p => state.key_status?.providers?.[p]?.configured);
    const variants: Option[] = [{ label: de.seriesAsSubmitted, providers: [...new Set<string>(s.items.map((item: RecordData) => item.snapshot.parameters.provider))] }];
    for (const model of state.models) {
      if (model.provider === "test" && hasPaid) continue;
      for (const resolution of model.resolutions || ["1K"]) for (const quality of model.qualities || [undefined]) {
        variants.push({ label: `${model.name || model.id} · ${resolution}${quality ? ` · ${quality}` : ""}`, providers: [model.provider], selection: {provider:model.provider, model:model.id, resolution, ...(quality ? {quality} : {})} });
      }
    }
    Promise.all(variants.map(async option => {
      try { return {...option, quote: await rpc("series.quote", {id:s.id, ...(option.selection ? {selection:option.selection} : {})})}; }
      catch(e) { return {...option, error:message(e)}; }
    })).then(result => { if (live) { setOptions(result); if (!result.some(o => o.quote)) setQuoteError(de.seriesQuoteFailed); } })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, [s.id, s.status, s.request_fingerprint, state.pricing_revision, keySignature]);
  const chosen = options[choice];
  const current = s.status === "pending" && chosen?.quote && !loading ? {...s, ...chosen.quote} : s;
  const overLimit = !!chosen?.quote && current.estimated_microusd > chosen.quote.job_limit_microusd;
  const ready = !loading && !!chosen?.quote && chosen.providers.every(p => p === "test" || state.key_status?.providers?.[p]?.configured) && (!overLimit || extendLimit);
  return (        <article className="series-approval-card">
          <header><div><span className="eyebrow">{names[s.status] || s.status}</span><h3>{s.description}</h3><p className="muted">{s.project_name}</p></div><strong className="series-total">{money(current.estimated_microusd)}<small>{de.estimated} · {s.count} {s.count === 1 ? de.image : de.seriesImages}</small></strong></header>
          <dl className="series-origin">
            <div><dt>{de.seriesOrigin}</dt><dd>{({rest:"REST API", mcp:"MCP", local:de.seriesLocal, automation:de.seriesAutomation} as Record<string,string>)[s.origin.channel] || de.seriesAutomation}</dd></div>
            <div><dt>{de.seriesAccess}</dt><dd>{s.origin.name || de.seriesLocal}{s.origin.recipient ? ` · ${s.origin.recipient}` : ""}</dd></div>
            <div><dt>{de.seriesReceived}</dt><dd>{new Date(s.created_at).toLocaleString(locale())}</dd></div>
          </dl>
          {s.origin.purpose && <p className="muted">{s.origin.purpose}</p>}
          {s.status === "pending" && <div className="series-model-review">
            <h4>{de.seriesModelChoice}</h4><p className="muted">{de.seriesModelHint}</p>
            {loading && <p role="status">{de.seriesPricingLoading}</p>}
            <ErrorBox error={quoteError} />
            {!loading && <div className="series-quote-table"><table><thead><tr><th>{de.seriesModelChoice}</th><th>{de.seriesPerImage}</th><th>{de.seriesTotalEstimate}</th></tr></thead><tbody>
              {options.map((option, index) => {
                const available = option.providers.every(provider => provider === "test" || state.key_status?.providers?.[provider]?.configured);
                const amounts = option.quote?.items.map((item: RecordData) => item.estimated_microusd) || [];
                const min = Math.min(...amounts), max = Math.max(...amounts);
                return <tr key={option.label} className={index === choice ? "selected" : ""}>
                  <td><label><input type="radio" name={`series-model-${s.id}`} checked={index === choice} disabled={!!busy || !available || !option.quote} onChange={() => { setChoice(index); setExtendLimit(false); }} />
                  <span>{option.label}{!available && <small>{de.seriesKeyMissing}</small>}{option.error && <small>{translateError(option.error)}</small>}</span></label></td>
                  <td>{amounts.length ? `${money(min)}${max !== min ? ` – ${money(max)}` : ""}` : "—"}</td><td>{option.quote ? money(option.quote.estimated_microusd) : "—"}</td>
                </tr>;
              })}
            </tbody></table></div>}
            {chosen?.quote && <p className="muted">{de.seriesPriceBasis}: {[...new Set(chosen.quote.items.map((item: RecordData) => `${item.snapshot.price_snapshot.version} (${item.snapshot.price_snapshot.checked_at || "—"})`))].join(", ")}</p>}
            {overLimit && <label className="series-budget-consent"><input type="checkbox" checked={extendLimit} disabled={!!busy} onChange={e => setExtendLimit(e.target.checked)} /><span>{de.seriesOneTimeLimit} <strong>{money(current.estimated_microusd)}</strong>. {de.seriesGlobalLimit} {money(chosen?.quote?.job_limit_microusd)}. {de.seriesProjectLimitRemains}</span></label>}
          </div>}
          <details><summary>{de.seriesImageList} ({s.count})</summary>
            <ol className="series-items">{current.items.map((item: RecordData, index: number) => <li key={index}>
              <strong>{item.snapshot.parameters.description}</strong><span>{money(item.estimated_microusd)} · {item.snapshot.parameters.provider} · {item.snapshot.parameters.model}</span>
              <small>{item.snapshot.parameters.aspect_ratio} · {item.snapshot.parameters.resolution}{item.snapshot.parameters.quality ? ` · ${item.snapshot.parameters.quality}` : ""} · {item.snapshot.references.length} {de.references}</small>
              <details><summary>{de.prompt}</summary><pre>{item.snapshot.prompt}</pre></details>
            </li>)}</ol>
          </details>
          {Object.keys(s.job_status_counts).length > 0 && <p className="series-progress" aria-live="polite">{Object.entries(s.job_status_counts).map(([status,count]) => `${count} × ${statuses[status] || status}`).join(" · ")}</p>}
          {actionError && <div className="series-action-feedback" ref={feedback} tabIndex={-1}><ErrorBox error={actionError} /></div>}
          {s.status === "pending" && <>
            {current.warning && <p className="warning">{de.seriesWarning}</p>}
            <p className="muted">{de.seriesApprovalScope}</p>
            <div className="form-actions">
              <button className="button ghost" disabled={!!busy} onClick={() => void action("series.reject",s.id)}>{de.seriesReject}</button>
              <button className="button primary" disabled={!!busy || !ready} onClick={() => void action("series.approve",s.id, { ...(chosen?.selection ? {selection: chosen.selection} : {}), expected_quote: chosen?.quote?.quote_id, ...(overLimit && extendLimit ? {series_limit_microusd: current.estimated_microusd} : {}) })}>{busy === s.id ? de.loading : `${de.seriesApprove} · ${s.count} ${s.count === 1 ? de.image : de.seriesImages}`}</button>
            </div>
          </>}
          {s.status === "approved" && (s.job_status_counts.queued || s.job_status_counts.running || s.job_status_counts.cancelling) && <button className="button ghost" disabled={!!busy} onClick={() => void action("series.cancel",s.id)}>{de.seriesStop}</button>}
          {s.jobs.some((job: RecordData) => job.session_key_required && job.status === "queued") && <p className="muted">{de.seriesSessionRequired}</p>}
          {s.status === "approved" && <button className="button small" disabled={!!busy} onClick={() => void action("series.get", s.id)}>{de.seriesOpenProject}</button>}
          {s.job_status_counts.completed > 0 && <button className="button small" disabled={!!busy} onClick={() => void action("series.export", s.id)}>{de.exportSeries}</button>}
          {s.approved_at && <small className="muted">{de.seriesApprovedAt}: {new Date(s.approved_at).toLocaleString(locale())}</small>}
        </article>);
}
