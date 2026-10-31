import { localizedRecord } from "./i18n";
import { locale } from "./i18n";
import { t } from "./i18n";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Copy, KeyRound, Plus, ShieldCheck } from "lucide-react";
import { rpc, message } from "./api";
import { ErrorBox, Field } from "./components";
import { de } from "./i18n";

type Access = {
  id: string;
  name: string;
  recipient: string;
  purpose: string;
  scope: "rest" | "mcp" | "both";
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
  active: boolean;
};
type Draft = Pick<Access, "name" | "recipient" | "purpose" | "scope">;
const emptyDraft = (): Draft => ({ name: "", recipient: "", purpose: "", scope: "rest" });
const scopeNames = localizedRecord({ rest: de.accessRest, mcp: de.accessMcp, both: de.accessBoth });

function dateLabel(value: string | null) {
  if (!value) return de.accessNever;
  const normalized = value.includes("T") ? value : `${value.replace(" ", "T")}Z`;
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? de.unknown : date.toLocaleString(locale(), { dateStyle: "medium", timeStyle: "short" });
}

export default function AccessTokens() {
  const [accesses, setAccesses] = useState<Access[]>([]);
  const [history, setHistory] = useState(false);
  const [expanded, setExpanded] = useState<string[]>([]);
  const [limit, setLimit] = useState(10);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft>(emptyDraft);
  const [secret, setSecret] = useState<{ token: string; id: string; name: string } | null>(null);
  const secretRef = useRef<HTMLDivElement>(null);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    rpc<Access[]>("tokens.list").then((result) => {
      if (live.current) { setAccesses(result); setLoaded(true); }
    }).catch((cause) => { if (live.current) setError(message(cause)); });
    return () => { live.current = false; };
  }, []);
  useEffect(() => {
    if (secret) {
      secretRef.current?.scrollIntoView({ block: "nearest" });
      secretRef.current?.focus();
    }
  }, [secret]);

  async function reload() {
    const result = await rpc<Access[]>("tokens.list");
    if (live.current) { setAccesses(result); setLoaded(true); }
  }
  async function act(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError(""); setNotice("");
    try { await action(); }
    catch (cause) { if (live.current) setError(message(cause)); }
    finally { if (live.current) setBusy(false); }
  }
  function edit(access?: Access) {
    setEditing(access?.id || "new");
    setDraft(access ? { name: access.name, recipient: access.recipient, purpose: access.purpose, scope: access.scope } : emptyDraft());
    setNotice(""); setError("");
  }
  async function save(event: FormEvent) {
    event.preventDefault();
    await act(async () => {
      if (editing === "new") {
        const result = await rpc<{ token: string; access: Access }>("tokens.create", draft);
        if (!live.current) return;
        // The raw token stays in this mounted dialog only; inventory RPCs never return it.
        setSecret({ token: result.token, id: result.access.id, name: result.access.name });
      } else {
        await rpc("tokens.update", { id: editing, name: draft.name, recipient: draft.recipient, purpose: draft.purpose });
        if (live.current) setNotice(de.accessSaved);
      }
      if (live.current) { setEditing(null); setDraft(emptyDraft()); }
      await reload();
    });
  }
  async function revoke(access: Access) {
    await act(async () => {
      await rpc("tokens.revoke", { id: access.id });
      if (live.current) {
        if (secret?.id === access.id) setSecret(null);
        if (editing === access.id) setEditing(null);
        setNotice(de.accessRevokedNotice);
      }
      await reload();
    });
  }
  async function copy() {
    if (!secret) return;
    try { await navigator.clipboard.writeText(secret.token); setNotice(de.copied); }
    catch { setError(de.accessCopyFailed); }
  }
  return <section className="access-panel" aria-label={de.accessTitle}>
    <div className="access-heading"><div><h3><KeyRound size={17} />{de.accessTitle}</h3><p>{de.accessIntro}</p></div>
      <button type="button" className="button small" disabled={busy || !loaded || Boolean(secret) || editing !== null} onClick={() => edit()}><Plus size={15} />{de.accessCreate}</button>
    </div>
    <ErrorBox error={error} />
    {notice && <p className="success" role="status">{notice}</p>}
    {secret && <div className="access-secret" ref={secretRef} tabIndex={-1} aria-label={de.accessSecretTitle}>
      <h4>{de.accessSecretTitle}: {secret.name}</h4><p>{de.accessSecretHint}</p>
      <Field label={de.accessSecretValue}><textarea readOnly autoComplete="off" spellCheck={false} rows={2} value={secret.token} onFocus={(event) => event.target.select()} /></Field>
      <div className="button-row"><button type="button" className="button small" onClick={() => void copy()}><Copy size={14} />{de.accessCopy}</button><button type="button" className="button small" onClick={() => { setSecret(null); setNotice(de.accessHidden); }}>{de.accessHide}</button></div>
    </div>}
    {editing !== null && <form className="access-editor" onSubmit={(event) => void save(event)}>
      <h4>{editing === "new" ? de.accessCreate : de.accessEdit}</h4>
      <div className="field-row"><Field label={de.accessName}><input autoFocus required maxLength={120} value={draft.name} disabled={busy} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder={de.accessNameExample} /></Field>
        <Field label={de.accessRecipient}><input required maxLength={160} value={draft.recipient} disabled={busy} onChange={(event) => setDraft({ ...draft, recipient: event.target.value })} placeholder={de.accessRecipientExample} /></Field></div>
      <Field label={de.accessPurpose}><input maxLength={1000} value={draft.purpose} disabled={busy} onChange={(event) => setDraft({ ...draft, purpose: event.target.value })} /></Field>
      <Field label={de.accessScope} hint={de.accessScopeHint}><select value={draft.scope} disabled={busy || editing !== "new"} onChange={(event) => setDraft({ ...draft, scope: event.target.value as Access["scope"] })}>{Object.entries(scopeNames).map(([scope, label]) => <option key={scope} value={scope}>{label}</option>)}</select></Field>
      <div className="button-row"><button type="submit" className="button primary" disabled={busy || !draft.name.trim() || !draft.recipient.trim()}>{busy ? de.busy : editing === "new" ? de.accessGenerate : de.accessSave}</button><button type="button" className="button ghost" disabled={busy} onClick={() => setEditing(null)}>{de.cancel}</button></div>
    </form>}
    {!loaded && !error && <p className="hint">{de.loading}</p>}
    {loaded && accesses.length === 0 && <p className="access-empty">{de.accessEmpty}</p>}
    <div className="button-row access-filters"><button type="button" className="button small" aria-pressed={!history} onClick={()=>{setHistory(false);setLimit(10);}}>{de.accessActive} ({accesses.filter(a=>a.active).length})</button><button type="button" className="button small" aria-pressed={history} onClick={()=>{setHistory(true);setLimit(10);}}>{de.accessRevoked} ({accesses.filter(a=>!a.active).length})</button></div>
    {loaded && !history && accesses.length > 0 && !accesses.some(a=>a.active) && <p className="hint">{de.accessNoActive}</p>}
    {history && <p className="hint">{de.accessHistoryHint}</p>}
    <div className="access-list">{accesses.filter(a=>history ? !a.active : a.active).slice(0,limit).map((access) => <article className={`access-record ${access.active ? "" : "revoked"}`} key={access.id} aria-label={`${de.accessRecord}: ${access.name}`}>
      <div className="access-record-heading"><h4>{access.name}</h4><span className={access.active ? "access-active" : "hint"}><ShieldCheck size={13} />{access.active ? de.accessActive : de.accessRevoked}</span></div>
      <p className="access-recipient">{access.recipient || de.accessUnassigned}<span>{scopeNames[access.scope]}</span></p>
      {expanded.includes(access.id) && <>{access.purpose && <p className="access-purpose">{access.purpose}</p>}
      <dl className="access-dates"><div><dt>{de.accessCreated}</dt><dd>{dateLabel(access.created_at)}</dd></div><div><dt>{de.accessUsed}</dt><dd>{dateLabel(access.last_used_at)}</dd></div>{access.revoked_at && <div><dt>{de.accessRevokedAt}</dt><dd>{dateLabel(access.revoked_at)}</dd></div>}</dl>
      <code>{access.id}</code></>}<div className="access-record-footer"><button type="button" className="text-button" aria-expanded={expanded.includes(access.id)} onClick={()=>setExpanded(ids=>ids.includes(access.id)?ids.filter(id=>id!==access.id):[...ids,access.id])}>{de.accessDetails}</button><div className="button-row"><button type="button" className="button small" disabled={busy || editing !== null} onClick={() => edit(access)}>{de.accessEdit}</button>{access.active && <button type="button" className="button small ghost danger" disabled={busy} onClick={() => void revoke(access)}>{de.accessRevoke}</button>}</div></div>
    </article>)}</div>{accesses.filter(a=>history ? !a.active : a.active).length > limit && <button className="button small" onClick={()=>setLimit(n=>n+10)}>{de.loadMore}</button>}
    <p className="hint access-mcp">{de.accessMcpHint} <code>FRAMESTUDIO_MCP_TOKEN</code>. {de.accessPrivacy}</p>
  </section>;
}
