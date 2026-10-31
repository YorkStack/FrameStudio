import { styleName } from "./i18n";
import { formatText } from "./i18n";
import { t } from "./i18n";
import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  Download,
  FolderOpen,
  Plus,
  Trash2,
  Check,
  KeyRound,
  ShieldCheck,
} from "lucide-react";
import { rpc, message, money, downloadExport, isDesktop, fileBase64, downloadJson, chooseDirectory } from "./api";
import { de } from "./i18n";
import { AssetImage, ErrorBox, Field, FormActions, Modal } from "./components";
import type { AppState, Asset, Project, RecordData } from "./types";
import PricingPanel from "./PricingPanel";
import AccessTokens from "./AccessTokens";
type EditorProps = { onClose: () => void; onSaved: () => Promise<void> | void };
export function ProjectEditor({
  project,
  styles,
  onClose,
  onSaved,
}: Omit<EditorProps, "onSaved"> & { project?: Project; styles: RecordData[]; onSaved: (saved: Project) => Promise<void> | void }) {
  const [value, setValue] = useState<RecordData>(
    project || {
      name: "",
      description: "",
      style_id: "",
      aspect_ratio: "1:1",
      budget_microusd: 10_000_000,
    },
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const field = (key: string, val: any) =>
    setValue((v) => ({ ...v, [key]: val }));
  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const saved = await rpc<Project>("project.save", value);
      await onSaved(saved);
      onClose();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={project ? de.projectDefaults : de.newProject}
      onClose={onClose}
    >
      <form onSubmit={save}>
        <Field label={de.projectName}>
          <input
            autoFocus
            required
            maxLength={200}
            value={value.name}
            onChange={(e) => field("name", e.target.value)}
          />
        </Field>
        <Field label={de.description}>
          <textarea
            rows={4}
            value={value.description}
            onChange={(e) => field("description", e.target.value)}
          />
        </Field>
        <div className="field-row">
          <Field label={de.style}>
            <select
              value={value.style_id || ""}
              onChange={(e) => field("style_id", e.target.value)}
            >
              <option value="">{de.none}</option>
              {styles.map((s) => (
                <option key={s.id} value={s.id}>
                  {styleName(s)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={de.aspect}>
            <select
              value={value.aspect_ratio || "1:1"}
              onChange={(e) => field("aspect_ratio", e.target.value)}
            >
              {["1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3"].map((r) => (
                <option key={r}>{r}</option>
              ))}
            </select>
          </Field>
        </div>
        <Field label={t("Standard-Ausgabeformat")}>
          <select
            value={value.preferences?.export_format || "png"}
            onChange={(e) =>
              field("preferences", {
                ...value.preferences,
                export_format: e.target.value,
              })
            }
          >
            {[
              "original",
              "png",
              "jpeg",
              "webp",
              "tiff",
              "html",
              "pdf",
              "pptx",
              "sprite",
              "zip",
            ].map((format) => (
              <option key={format} value={format}>
                {format.toUpperCase()}
              </option>
            ))}
          </select>
        </Field>
        <Field label={de.background}>
          <input
            type="color"
            value={value.preferences?.background || "#ffffff"}
            onChange={(e) =>
              field("preferences", {
                ...value.preferences,
                background: e.target.value,
              })
            }
          />
        </Field>
        <Field label={de.budget}>
          <input
            type="number"
            min="0"
            step="0.01"
            value={(value.budget_microusd ?? 10_000_000) / 1_000_000}
            onChange={(e) =>
              field(
                "budget_microusd",
                Math.round(Number(e.target.value) * 1_000_000),
              )
            }
          />
        </Field>
        <ErrorBox error={error} />
        <FormActions onClose={onClose} busy={busy} />
      </form>
    </Modal>
  );
}
export function StyleEditor({
  style,
  onClose,
  onSaved,
}: EditorProps & { style?: RecordData }) {
  const [value, setValue] = useState<RecordData>(
    style || {
      name: "",
      blocks: { medium: "", lines: "", palette: "", light: "", texture: "" },
    },
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      await rpc("style.save", { ...value, preview_prompt: value.preview_prompt?.trim() || undefined });
      await onSaved();
      onClose();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={
        style
          ? `${de.style} · ${de.version} ${style.version || 1}`
          : de.newStyle
      }
      onClose={onClose}
    >
      <form onSubmit={save}>
        <Field label={de.name}>
          <input
            autoFocus
            required
            value={value.name}
            onChange={(e) => setValue({ ...value, name: e.target.value })}
          />
        </Field>
        {(["medium", "lines", "palette", "light", "texture"] as const).map(
          (k) => (
            <Field key={k} label={de[k]}>
              <textarea
                rows={2}
                value={value.blocks?.[k] || ""}
                onChange={(e) =>
                  setValue({
                    ...value,
                    blocks: { ...value.blocks, [k]: e.target.value },
                  })
                }
              />
            </Field>
          ),
        )}
        <Field label={de.stylePreviewPrompt} hint={de.stylePreviewPromptHint}>
          <textarea
            rows={5}
            required={Boolean(style)}
            maxLength={12000}
            value={value.preview_prompt || ""}
            onChange={(e) => setValue({ ...value, preview_prompt: e.target.value })}
            placeholder={t("Eigenes Motiv für den Stilvergleich. Neue Stile erhalten ohne Eingabe das Standardmotiv.")}
          />
        </Field>
        <p className="hint">{de.immutable}</p>
        <ErrorBox error={error} />
        <FormActions onClose={onClose} busy={busy} />
      </form>
    </Modal>
  );
}
export function CharacterEditor({
  character,
  projectId,
  assets,
  onClose,
  onSaved,
}: EditorProps & {
  character?: RecordData;
  projectId: string;
  assets: Asset[];
}) {
  const [value, setValue] = useState<RecordData>(
    character || {
      project_id: projectId,
      name: "",
      description: "",
      features: "",
      clothing: "",
      colors: "",
      binding: [],
      reference_ids: [],
      approved: false,
    },
  );
  const [available,setAvailable]=useState<Asset[]>(assets);
  const [filter,setFilter]=useState("all");
  const [page,setPage]=useState(0);
  const importInput=useRef<HTMLInputElement>(null);
  const [loading,setLoading]=useState(false);
  useEffect(()=>{let live=true;(async()=>{setLoading(true);try{let all:Asset[]=[];let offset=0;while(true){const r=await rpc("asset.list",{project_id:projectId,offset,limit:100});all.push(...r.items);offset+=r.items.length;if(offset>=r.total||!r.items.length)break;}if(live)setAvailable(all);}catch(e){if(live)setError(message(e));}finally{if(live)setLoading(false);}})();return()=>{live=false;};},[projectId]);
  const filtered=available.filter(a=>filter === "all" || (filter === "import" ? a.provider === "import" : a.provider !== "import"));
  async function importPhoto(file?:File){if(!file)return;setLoading(true);setError("");try{const a=await rpc<Asset>("asset.import",{project_id:projectId,name:file.name,description:file.name,data_base64:await fileBase64(file)});setAvailable(v=>[a,...v]);setValue(v=>({...v,reference_ids:[...v.reference_ids,a.id]}));setFilter("import");setPage(0);}catch(e){setError(message(e));}finally{setLoading(false);if(importInput.current)importInput.current.value="";}}
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const set = (key: string, val: any) =>
    setValue((v) => ({ ...v, [key]: val }));
  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      await rpc("character.save", value);
      await onSaved();
      onClose();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={character ? de.character : de.newCharacter}
      onClose={onClose}
      wide
    >
      <form onSubmit={save}>
        <div className="editor-columns">
          <div>
            <Field label={de.name}>
              <input
                autoFocus
                required
                value={value.name}
                onChange={(e) => set("name", e.target.value)}
              />
            </Field>
            {(["description", "features", "clothing", "colors"] as const).map(
              (k) => (
                <Field key={k} label={de[k]}>
                  <textarea
                    rows={2}
                    value={value[k] || ""}
                    onChange={(e) => set(k, e.target.value)}
                  />
                </Field>
              ),
            )}
            <Field label={de.binding}>
              <textarea
                rows={3}
                value={(value.binding || []).join("\n")}
                onChange={(e) => set("binding", e.target.value.split("\n"))}
              />
            </Field>
          </div>
          <div>
            <h3>{de.referenceChoose}</h3>
            <input ref={importInput} type="file" hidden accept="image/png,image/jpeg,image/webp,image/tiff" onChange={e=>void importPhoto(e.target.files?.[0])}/>
            <button className="button" type="button" disabled={loading || value.reference_ids.length>=8} onClick={()=>importInput.current?.click()}>{de.characterReferenceImport}</button>
            <Field label={de.characterReferencesFilter}><select value={filter} onChange={e=>{setFilter(e.target.value);setPage(0);}}><option value="all">{de.characterReferencesAll}</option><option value="import">{de.characterReferencesImported}</option><option value="generated">{de.characterReferencesGenerated}</option></select></Field>
            {loading && <p role="status">{de.loading}</p>}
            {available.length === 0 && <p className="hint">{de.noReference}</p>}
            <div className="reference-grid">
              {filtered.slice(page*24,(page+1)*24).map((a) => (
                <button
                  type="button"
                  aria-label={a.description || a.id.slice(0, 8)}
                  aria-pressed={value.reference_ids.includes(a.id)}
                  className={`reference-option ${value.reference_ids.includes(a.id) ? "selected" : ""}`}
                  key={a.id}
                  disabled={!value.reference_ids.includes(a.id) && value.reference_ids.length>=8}
                  onClick={()=>setValue(v=>{const ids=v.reference_ids.includes(a.id)?v.reference_ids.filter((id:string)=>id!==a.id):[...v.reference_ids,a.id];return {...v,reference_ids:ids,approved:ids.length ? v.approved : false};})}
                >
                  <AssetImage id={a.id} alt={a.description} />
                  <span>{a.description || a.id.slice(0, 8)}</span>
                  {value.reference_ids.includes(a.id) && <Check size={16} />}
                </button>
              ))}
            </div>
            {filtered.length>24 && <div className="button-row"><button type="button" className="button" disabled={!page} onClick={()=>setPage(page-1)}>{de.previous}</button><span>{page+1} / {Math.ceil(filtered.length/24)}</span><button type="button" className="button" disabled={(page+1)*24>=filtered.length} onClick={()=>setPage(page+1)}>{de.next}</button></div>}
            <p className="hint">{value.reference_ids.length}{t("/ 8 Referenzen ausgewählt")}</p>
            <Field label={de.characterPortrait} hint={de.characterPortraitHint}><select value={value.portrait_asset_id || ""} onChange={e=>set("portrait_asset_id",e.target.value)}><option value="">{t("Automatisch: erstes erzeugtes Bild dieses Charakters")}</option>{available.filter(a=>a.provider!=="import").map(a=><option key={a.id} value={a.id}>{a.description || a.id}</option>)}</select></Field>
            {value.portrait_asset_id && <div className="character-portrait-preview"><AssetImage id={value.portrait_asset_id} alt={de.characterPortrait}/></div>}
            <label className="check-field">
              <input
                type="checkbox"
                checked={Boolean(value.approved)}
                onChange={(e) => set("approved", e.target.checked)}
              />
              {de.approved}
            </label>
            <p className="hint">{de.consistency}</p>
          </div>
        </div>
        <ErrorBox error={error} />
        <FormActions onClose={onClose} busy={busy} />
      </form>
    </Modal>
  );
}
export function SettingsEditor({
  state,
  onClose,
  onSaved,
}: EditorProps & { state: AppState }) {
  const [value, setValue] = useState<RecordData>({ ...state.settings });
  const [storage, setStorage] = useState(state.data_dir || "");
  const [key, setKey] = useState("");
  const [keyProvider, setKeyProvider] = useState("gemini");
  const [keyMode, setKeyMode] = useState(state.key_status?.providers?.gemini?.status === "session" ? "session" : "keychain");
  const [sessionProviders, setSessionProviders] = useState<Record<string, boolean>>({});
  const [keyStatus, setKeyStatus] = useState(state.key_status);
  useEffect(() => {
    setKeyStatus(state.key_status);
    setSessionProviders(Object.fromEntries(["gemini", "openai", "anthropic"].map(
      (provider) => [provider, state.key_status?.providers?.[provider]?.status === "session"]
    )));
  }, [state.key_status]);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const mounted = useRef(true);
  const [keyPending, setKeyPending] = useState(false);
  const [keyError, setKeyError] = useState("");
  const [keyNotice, setKeyNotice] = useState("");
  const [keyChecks, setKeyChecks] = useState<Record<string, { active?: boolean; keychain?: boolean }>>({});
  const keyFeedback = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (keyError || keyNotice) keyFeedback.current?.scrollIntoView({ block: "nearest" });
  }, [keyError, keyNotice]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  const set = (key: string, val: any) =>
    setValue((v) => ({ ...v, [key]: val }));
  const selectedKeyStatus =
    keyStatus?.providers?.[keyProvider] ??
    (keyProvider === "gemini" ? keyStatus : undefined);
  const keyed =
    typeof selectedKeyStatus === "boolean"
      ? selectedKeyStatus
      : Boolean(selectedKeyStatus?.configured);
  async function act(fn: () => Promise<void>, keyAction = false) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    if (keyAction) {
      setKeyPending(true);
      setKeyError("");
      setKeyNotice("");
    } else {
      setError("");
      setNotice("");
    }
    try {
      await fn();
    } catch (e) {
      if (mounted.current) {
        if (keyAction) setKeyError(message(e));
        else setError(message(e));
      }
    } finally {
      busyRef.current = false;
      if (mounted.current) {
        setBusy(false);
        setKeyPending(false);
      }
    }
  }
  function checkKey(source: "active" | "keychain") {
    void act(async () => {
      const field = source === "keychain" || !sessionProviders[keyProvider] ? "keychain" : "active";
      try {
        await rpc("secret.check", { provider: keyProvider, ...(source === "keychain" ? {source} : {}) });
        if (mounted.current) {
          setKeyChecks(checks => ({...checks, [keyProvider]: {...checks[keyProvider], [field]: true}}));
          setKeyNotice(source === "keychain" ? de.keychainReadable : de.keyReadable);
        }
      } catch (cause) {
        if (mounted.current) setKeyChecks(checks => ({...checks, [keyProvider]: {...checks[keyProvider], [field]: false}}));
        throw cause;
      }
    }, true);
  }
  async function save(e: FormEvent) {
    e.preventDefault();
    await act(async () => {
      await rpc("settings.save", value);
      await onSaved();
      onClose();
    });
  }
  const [section, setSection] = useState("providers");
  const [accessVisited, setAccessVisited] = useState(false);
  const [pricingVisited, setPricingVisited] = useState(false);
  const sections = [
    ["about", de.appInfo, de.appInfoHint],
    ["general", de.settingsGeneral, de.settingsGeneralHint],
    ["providers", de.settingsProviders, de.settingsProvidersHint],
    ["storage", de.settingsStorage, de.settingsStorageHint],
    ["budget", de.settingsBudget, de.settingsBudgetHint],
    ["pricing", de.pricingTitle, de.settingsPricingHint],
    ["access", de.settingsAccess, de.settingsAccessHint],
  ];
  const selectedSection = sections.find(entry => entry[0] === section)!;
  const dirty = JSON.stringify(value) !== JSON.stringify(state.settings);
  function navigate(next: string) {
    setSection(next);
    if (next === "access") setAccessVisited(true);
    if (next === "pricing") setPricingVisited(true);
  }
  return (
    <Modal title={de.settings} onClose={onClose} wide>
      <div className="settings-layout">
        <nav className="settings-nav" aria-label={de.settingsSections}>
          {sections.map(([id, label]) => <button type="button" key={id} aria-current={section === id ? "page" : undefined} onClick={() => navigate(id)}>{label}</button>)}
        </nav>
        <div className="settings-content">
          <header className="settings-section-heading"><h3>{selectedSection[1]}</h3><p className="muted">{selectedSection[2]}</p></header>
          <section hidden={section !== "about"}><h3>FrameStudio {state.app_version || "0.3.0"}</h3><p>{de.sbomHint}</p><button className="button" disabled={busy} onClick={()=>void act(async()=>{const bom=await rpc("app.sbom");if(isDesktop()){const out=await rpc("app.sbom.export");setNotice(formatText("{0} Komponenten gespeichert: {1}",bom.components.length,out.path));await rpc("export.reveal",{id:out.id});}else{downloadJson(bom,`FrameStudio-${state.app_version||"0.3.0"}-sbom.cdx.json`);setNotice(formatText("{0} Komponenten exportiert.",bom.components.length));}})}>{de.sbomExport}</button></section>
          <form id="settings-preferences" onSubmit={save}>
            <section className="settings-section" hidden={section !== "general"} aria-label={de.settingsGeneral}>
            <div className="field-row">
              <Field label={de.language}><select value={value.language || "de"} onChange={e=>set("language",e.target.value)}><option value="de">Deutsch</option><option value="en">English</option></select></Field>
              <Field label={de.theme}>
                <select
                  value={value.theme || "system"}
                  onChange={(e) => set("theme", e.target.value)}
                >
                  <option value="system">{de.system}</option>
                  <option value="light">{de.lightTheme}</option>
                  <option value="dark">{de.dark}</option>
                </select>
              </Field>
              <Field label={t("Bedienmodus")}>
                <select
                  value={value.mode || "simple"}
                  onChange={(e) => set("mode", e.target.value)}
                >
                  <option value="simple">{de.simple}</option>
                  <option value="expert">{de.expert}</option>
                </select>
              </Field>
            </div>

            </section>
            <section className="settings-section" hidden={section !== "storage"} aria-label={de.settingsStorage}>
            {isDesktop() && <button type="button" className="button small" disabled={busy} onClick={()=>void act(async()=>{const path=await chooseDirectory();if(path)setStorage(path);})}><FolderOpen size={16}/>{de.chooseFolder}</button>}
            <Field label={de.storage} hint={formatText("Aktiv: {0}",state.data_dir)}>
              <input
                value={storage}
                onChange={(e) => setStorage(e.target.value)}
              />
            </Field>
            <button
              type="button"
              className="button small"
              disabled={busy || storage === state.data_dir || !storage.trim()}
              onClick={() =>
                void act(async () => {
                  await rpc("storage.configure", { path: storage });
                  setNotice(
                    t("Datenverzeichnis für den nächsten Start gespeichert. Vorhandene Daten bleiben am bisherigen Ort."),
                  );
                })
              }
            >{t("Datenbestand beim nächsten Start wechseln")}</button>
            <p className="hint">{t("Öffnet nach Neustart einen vorhandenen oder neuen Datenbestand. Bestehende Projekte bleiben im bisherigen Verzeichnis erhalten.")}</p>
            <Field label={de.exportDir}>
              <input
                value={value.export_dir || ""}
                onChange={(e) => set("export_dir", e.target.value)}
                placeholder={t("Standard: Datenverzeichnis / exports")}
              />
            </Field>
            <Field label={de.naming}>
              <input
                value={
                  value.naming_template ||
                  "{projekt}_{beschreibung}_{nummer:04}"
                }
                onChange={(e) => set("naming_template", e.target.value)}
              />
            </Field>
            <p className="hint">
              {"{charakter} · {pose} · {ausdruck} · {datum} · {variante}"}
            </p>

            </section>
            <section className="settings-section" hidden={section !== "budget"} aria-label={de.settingsBudget}>
            <h3>{de.projectCosts}</h3>
            {(
              [
                ["job_limit_microusd", de.jobLimit, 1_000_000],
                ["project_limit_microusd", de.projectLimit, 10_000_000],
                ["warning_microusd", de.warningLimit, 500_000],
              ] as const
            ).map(([key, label, fallback]) => (
              <Field key={key} label={label}>
                <input
                  type="number"
                  min="0"
                  step="0.01"
                  value={(value[key] ?? fallback) / 1_000_000}
                  onChange={(e) =>
                    set(key, Math.round(Number(e.target.value) * 1_000_000))
                  }
                />
              </Field>
            ))}
            <Field label={de.concurrency}>
              <select
                value={value.concurrency || 1}
                onChange={(e) => set("concurrency", Number(e.target.value))}
              >
                <option value={1}>1</option>
              </select>
            </Field>
            <p className="hint">{de.costNotice}</p>

            </section>
            <section className="settings-section" hidden={section !== "access"} aria-label={de.settingsConnections}>
            <h3>REST & MCP</h3>
            <label className="check-field">
              <input
                type="checkbox"
                checked={Boolean(value.rest_enabled)}
                onChange={(e) => set("rest_enabled", e.target.checked)}
              />
              {de.rest}
            </label>
            <label className="check-field">
              <input
                type="checkbox"
                checked={Boolean(value.mcp_enabled)}
                onChange={(e) => set("mcp_enabled", e.target.checked)}
              />
              {de.mcp}
            </label>
            <label className="check-field">
              <input
                type="checkbox"
                checked={Boolean(value.automation_approved)}
                onChange={(e) => set("automation_approved", e.target.checked)}
              />
              {de.automation}
            </label>
            <p className="hint">
              {de.automatedHint}{" "}
              {value.automation_approved && de.automationRisk}
            </p>

            </section>
          </form>
          <div className="settings-section" hidden={section !== "providers"}>
            <section aria-label={de.keyManagement} aria-busy={keyPending}>
            <h3>
              <KeyRound size={17} /> {de.key}
            </h3>
            <div className="settings-key-summary">{["gemini", "openai", "anthropic"].map((provider) => {
              const unavailable = keyStatus?.providers?.[provider]?.status === "unavailable";
              const configured =
                provider === keyProvider
                  ? keyed
                  : Boolean(keyStatus?.providers?.[provider]?.configured);
              const checked = keyChecks[provider];
              const failed = checked?.keychain === false || checked?.active === false;
              const checkedLabel = checked?.keychain === false ? de.keychainCheckFailed : checked?.keychain === true ? de.keychainCheckPassed : null;
              return (
                <p
                  key={provider}
                  className={`key-status ${failed ? "warning" : configured ? "success" : ""}`}
                >
                  <ShieldCheck size={16} />
                  {provider === "gemini" ? de.gemini : provider === "openai" ? de.openai : de.anthropic}: {sessionProviders[provider] ? `${de.keySessionActive}${checkedLabel ? ` · ${checkedLabel}` : ""}` : checkedLabel || (unavailable ? de.keyUnavailable : configured ? de.keyStored : de.keyMissing)}
                </p>
              );
            })}</div>
            <Field label={de.keyProvider}>
              <select
                value={keyProvider}
                disabled={busy}
                onChange={(e) => {
                  setKey("");
                  setKeyProvider(e.target.value);
                  setKeyNotice("");
                  setKeyError("");
                }}
              >
                <option value="gemini">{de.gemini}</option>
                <option value="openai">{de.openai}</option>
                <option value="anthropic">{de.anthropic}</option>
              </select>
            </Field>
            <details className="key-help"><summary>{de.apiKeyHelp}</summary><p>{keyProvider === "gemini" ? de.apiKeyHelpGemini : keyProvider === "openai" ? de.apiKeyHelpOpenai : de.apiKeyHelpAnthropic}</p><p className="hint">{de.apiKeyHelpPrivacy}</p><button type="button" className="button small" onClick={()=>void act(async()=>{await rpc("help.provider",{provider:keyProvider});})}>{de.apiKeyPortal}</button></details>
            {keyProvider === "openai" && (
              <p className="hint">{de.openaiApiHint}</p>
            )}
            {keyProvider === "anthropic" && <p className="hint">{de.anthropicKeyHint}</p>}
            <Field label={de.keyMode}>
              <select value={keyMode} disabled={busy} onChange={(e) => {
                setKeyMode(e.target.value); setKey(""); setKeyNotice(""); setKeyError("");
              }}>
                <option value="keychain">{de.keyModeKeychain}</option>
                <option value="session">{de.keyModeSession}</option>
              </select>
            </Field>
            {keyMode === "session" && <p className="hint">{de.keySessionHint}</p>}
            <Field
              label={keyProvider === "openai" ? de.openaiKey : keyProvider === "anthropic" ? de.anthropicKey : de.geminiKey}
              hint={keyMode === "session" ? de.keySessionFieldHint : de.keyHint}
            >
              <input
                type="password"
                autoComplete="new-password"
                value={key}
                disabled={busy}
                onChange={(e) => setKey(e.target.value)}
                placeholder={t("API-Schlüssel eingeben")}
              />
            </Field>
            <div className="button-row">
              <button
                type="button"
                className="button"
                disabled={!key.trim() || busy}
                onClick={() =>
                  void act(async () => {
                    const supplied = key;
                    setKey("");
                    setKeyChecks(checks => ({...checks, [keyProvider]: {}}));
                    const status = await rpc(keyMode === "session" ? "secret.session.set" : "secret.set", {
                      provider: keyProvider,
                      key: supplied,
                    });
                    if (mounted.current) {
                      if (keyMode === "session") setSessionProviders(status.providers);
                      else { setKeyStatus(status); setKeyChecks(checks => ({...checks, [keyProvider]: {keychain:true}})); }
                      setKeyNotice(keyMode === "session" ? de.keySessionActive : de.keyStoredVerified);
                    }
                    await onSaved();
                  }, true)
                }
              >
                {keyMode === "session" ? de.keySessionUse : de.keySave}
              </button>
              <button
                type="button"
                className="button ghost danger"
                disabled={busy || (keyMode === "session" ? !sessionProviders[keyProvider] : !keyed || sessionProviders[keyProvider])}
                onClick={() =>
                  void act(async () => {
                    setKey("");
                    setKeyChecks(checks => ({...checks, [keyProvider]: {}}));
                    const status = await rpc(keyMode === "session" ? "secret.session.remove" : "secret.delete", {
                      provider: keyProvider,
                    });
                    if (mounted.current) {
                      if (keyMode === "session") setSessionProviders(status.providers);
                      else setKeyStatus(status);
                      setKeyNotice(keyMode === "session" ? de.keySessionRemoved : de.keyDeleted);
                    }
                    await onSaved();
                  }, true)
                }
              >
                {keyMode === "session" ? de.keySessionRemove : de.keyDelete}
              </button>
            </div>
            <button type="button" className="button" disabled={busy} onClick={() => checkKey("active")}>{de.keyCheck}</button>
            <button type="button" className="button small" disabled={busy} onClick={() => checkKey("keychain")}>{de.keychainCheck}</button>
            <p className="hint">{de.keyCheckHint} {de.keychainCheckHint}</p>
            {sessionProviders[keyProvider] && keyMode === "keychain" && <p className="hint">{de.keySessionPriority}</p>}
            {keyPending && <p className="hint" role="status">{de.keyPending}</p>}
            <div ref={keyFeedback} className="key-feedback"><ErrorBox error={keyError} />
            {!keyPending && keyNotice && <p className="success" role="status">{keyNotice}</p>}</div>
            <p className="hint">{de.keychainLocation} <code>{keyProvider}-api-key</code>.</p><p className="hint">{de.keyNoDialogs}</p><details className="settings-help"><summary>{de.keyHelpTitle}</summary><p className="hint">{de.keyAccessHelp}</p></details>
            </section>

          </div>
          <div className="settings-section" hidden={section !== "pricing"}>{pricingVisited && <PricingPanel onUpdated={onSaved} expanded />}</div>
          <div className="settings-section" hidden={section !== "access"}>{accessVisited && <AccessTokens />}</div>
        </div>
      </div>
      <footer className="settings-footer">
        <div><ErrorBox error={error} />{notice && <p className="success" role="status">{notice}</p>}<p className="hint">{dirty ? de.settingsUnsaved : de.settingsImmediateHint}</p></div>
        <div className="button-row"><button type="button" className="button ghost" onClick={onClose}>{dirty ? de.cancel : de.settingsDone}</button><button type="submit" form="settings-preferences" className="button primary" disabled={busy || !dirty}>{busy ? de.busy : de.save}</button></div>
      </footer>
    </Modal>
  );
}
export function ExportEditor({
  project,
  assets,
  selected,
  initialFormat,
  seriesId,
  settings,
  onClose,
}: {
  project: Project;
  assets: Asset[];
  selected: string[];
  initialFormat?: string;
  seriesId?: string;
  settings: RecordData;
  onClose: () => void;
}) {
  const [value, setValue] = useState<RecordData>({
    project_id: project.id,
    asset_ids: selected,
    format: initialFormat || project.preferences?.export_format || "png",
    title: project.name,
    description: project.description,
    include_metadata: false,
    fit: "contain",
    background: project.preferences?.background || "#ffffff",
    layout: "single",
    cell_width: 512,
    cell_height: 512,
    columns: 4,
    padding: 16,
    page_width_pt: 960,
    page_height_pt: 540,
    naming_template:
      settings.naming_template || "{projekt}_{beschreibung}_{nummer:04}",
  });
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<RecordData | null>(null);
  const [jobId, setJobId] = useState("");
  const [filename, setFilename] = useState("");
  const [scopeLoading, setScopeLoading] = useState(!!seriesId);
  const [scopeError, setScopeError] = useState("");
  useEffect(() => {
    if (!seriesId) return;
    let live = true;
    setScopeLoading(true);
    void (async () => {
      try {
        const ids: string[] = [];
        let total = 0;
        do {
          const page = await rpc("asset.list", {project_id:project.id, series_id:seriesId, offset:ids.length, limit:200});
          total = page.total;
          if (!page.items.length) break;
          ids.push(...page.items.map((asset: Asset) => asset.id));
        } while (ids.length < total);
        if (live) {
          if (!ids.length) setScopeError(de.exportSeriesEmpty);
          else setValue(v => ({...v, asset_ids:ids}));
        }
      } catch(e) { if(live) setScopeError(message(e)); }
      finally { if(live) setScopeLoading(false); }
    })();
    return () => { live = false; };
  }, [seriesId, project.id]);
  const firstAssetId = value.asset_ids[0] || (!seriesId ? assets[0]?.id : undefined);
  useEffect(() => {
    let active = true;
    setFilename("");
    if (!firstAssetId) return;
    const timer = setTimeout(async () => {
      try {
        const result = await rpc("export.preview", {
          project_id: project.id,
          asset_id: firstAssetId,
          naming_template: value.naming_template,
          format: value.format,
        });
        if (active) setFilename(result.filename);
      } catch (error) {
        if (active) setFilename(message(error));
      }
    }, 200);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [firstAssetId, project.id, value.naming_template, value.format]);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const set = (key: string, val: any) =>
    setValue((v) => ({ ...v, [key]: val }));
  async function save(e: FormEvent) {
    e.preventDefault();
    if (scopeLoading || scopeError || (seriesId && !value.asset_ids.length)) return;
    setBusy(true);
    setError("");
    try {
      const job = await rpc("export.submit", {
        ...value,
        idempotency_key: crypto.randomUUID(),
      });
      setJobId(job.id);
      while (alive.current) {
        const current = await rpc("export.get", { id: job.id });
        if (current.status === "completed") {
          setResult(current.result);
          break;
        }
        if (["failed", "unknown", "cancelled"].includes(current.status))
          throw new Error(current.error || t("Export wurde nicht abgeschlossen."));
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
    } catch (e) {
      if (alive.current) setError(message(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  const document = ["pdf", "pptx", "html"].includes(value.format);
  const raster = ["png", "jpeg", "webp", "tiff"].includes(value.format);
  return (
    <Modal title={de.export} onClose={onClose}>
      <form onSubmit={save}>
        <p className="hint">
          {seriesId ? `${de.exportSeriesScope} · ${scopeLoading ? de.loading : value.asset_ids.length}` : selected.length
            ? `${selected.length} ${de.selectedCount}`
            : de.allProject}{" "}
          · {de.exportHint}
        </p>
        {seriesId && <p className="hint">{de.exportSeriesHint}</p>}
        <ErrorBox error={scopeError} />
        <Field label={de.exportFormat}>
          <select
            value={value.format}
            onChange={(e) => {
              set("format", e.target.value);
              setResult(null);
            }}
          >
            {[
              ["original", t("Originaldateien")],
              ["png", "PNG"],
              ["jpeg", "JPEG"],
              ["webp", "WebP"],
              ["tiff", "TIFF"],
              ["html", t("Offline-HTML-Galerie")],
              ["pdf", "PDF"],
              ["pptx", "PowerPoint / PPTX"],
              ["sprite", "Sprite-Sheet + JSON"],
              ["zip", t("ZIP-Projektpaket")],
            ].map(([id, label]) => (
              <option value={id} key={id}>
                {label}
              </option>
            ))}
          </select>
        </Field>
        {document && (
          <>
            <Field label={de.exportTitle}>
              <input
                value={value.title}
                onChange={(e) => set("title", e.target.value)}
              />
            </Field>
            <Field label={de.exportDescription}>
              <textarea
                rows={2}
                value={value.description}
                onChange={(e) => set("description", e.target.value)}
              />
            </Field>
            <Field label={de.exportLayout}>
              <select
                value={value.layout}
                onChange={(e) => set("layout", e.target.value)}
              >
                <option value="single">{de.single}</option>
                <option value="contact">{de.contact}</option>
              </select>
            </Field>
            <Field label={t("Seiten-/Folienformat")}>
              <select
                value={`${value.page_width_pt}x${value.page_height_pt}`}
                onChange={(e) => {
                  const [w, h] = e.target.value.split("x").map(Number);
                  setValue((v) => ({
                    ...v,
                    page_width_pt: w,
                    page_height_pt: h,
                  }));
                }}
              >
                <option value="960x540">{t("Breitbild 16:9")}</option>
                <option value="595x842">{t("A4 Hochformat")}</option>
                <option value="842x595">{t("A4 Querformat")}</option>
                <option value="720x720">{t("Quadrat")}</option>
              </select>
            </Field>
            <div className="field-row">
              <Field label={t("Seiten-/Folienbreite (pt)")}>
                <input
                  type="number"
                  min={300}
                  max={2000}
                  value={value.page_width_pt}
                  onChange={(e) => set("page_width_pt", Number(e.target.value))}
                />
              </Field>
              <Field label={t("Seiten-/Folienhöhe (pt)")}>
                <input
                  type="number"
                  min={300}
                  max={2000}
                  value={value.page_height_pt}
                  onChange={(e) =>
                    set("page_height_pt", Number(e.target.value))
                  }
                />
              </Field>
            </div>
          </>
        )}
        {raster && (
          <>
            <p className="hint">{de.dimensions}</p>
            <div className="field-row">
              <Field label={de.width}>
                <input
                  type="number"
                  min={1}
                  max={16384}
                  value={value.width || ""}
                  onChange={(e) =>
                    set(
                      "width",
                      e.target.value ? Number(e.target.value) : undefined,
                    )
                  }
                />
              </Field>
              <Field label={de.height}>
                <input
                  type="number"
                  min={1}
                  max={16384}
                  value={value.height || ""}
                  onChange={(e) =>
                    set(
                      "height",
                      e.target.value ? Number(e.target.value) : undefined,
                    )
                  }
                />
              </Field>
            </div>
            <Field label={de.resize}>
              <select
                value={value.fit}
                onChange={(e) => set("fit", e.target.value)}
              >
                {(["contain", "cover", "fill", "preserve"] as const).map(
                  (v) => (
                    <option key={v} value={v}>
                      {de[v]}
                    </option>
                  ),
                )}
              </select>
            </Field>
            <Field label={de.background}>
              <input
                type="color"
                value={value.background}
                onChange={(e) => set("background", e.target.value)}
              />
            </Field>
            <p className="hint">{de.upscale}</p>
          </>
        )}
        {value.format === "sprite" && (
          <>
            <div className="field-row">
              <Field label={`${de.cellSize} · ${de.width}`}>
                <input
                  type="number"
                  min={16}
                  max={4096}
                  value={value.cell_width}
                  onChange={(e) => set("cell_width", Number(e.target.value))}
                />
              </Field>
              <Field label={de.height}>
                <input
                  type="number"
                  min={16}
                  max={4096}
                  value={value.cell_height}
                  onChange={(e) => set("cell_height", Number(e.target.value))}
                />
              </Field>
            </div>
            <div className="field-row">
              <Field label={de.columns}>
                <input
                  type="number"
                  min={1}
                  max={32}
                  value={value.columns}
                  onChange={(e) => set("columns", Number(e.target.value))}
                />
              </Field>
              <Field label={de.padding}>
                <input
                  type="number"
                  min={0}
                  max={256}
                  value={value.padding}
                  onChange={(e) => set("padding", Number(e.target.value))}
                />
              </Field>
            </div>
            <p className="hint">{t("Reihenfolge: Auswahlreihenfolge, sonst Galerie. Figuren werden vollständig eingepasst.")}</p>
          </>
        )}
        {value.format === "original" && (
          <p className="hint">{t("Originaldateien können eingebettete Metadaten enthalten. Zum Entfernen als PNG/JPEG/WebP/TIFF exportieren.")}</p>
        )}
        {value.format !== "zip" && (
          <Field label={de.naming}>
            <input
              value={value.naming_template}
              onChange={(e) => set("naming_template", e.target.value)}
            />
          </Field>
        )}
        <p className="hint">
          {de.filenamePreview}: {filename || "—"}
        </p>
        <label className="check-field">
          <input
            type="checkbox"
            checked={value.include_metadata}
            onChange={(e) => set("include_metadata", e.target.checked)}
          />
          {de.includeMetadata}
        </label>
        {value.format === "zip" && <p className="hint">{de.restoreHint}</p>}
        <ErrorBox error={error} />
        {busy && jobId && (
          <p className="hint" role="status">{t("Exportauftrag wird verarbeitet ·")}{jobId}
          </p>
        )}
        {result ? (
          <div className="export-result">
            <Check size={22} />
            <h3>{de.exportDone}</h3>
            <p>{de.savedAt}</p>
            <code>{result.path}</code>
            {isDesktop() && (
              <button
                type="button"
                className="button"
                onClick={() =>
                  void rpc("export.reveal", { id: result.id }).catch((error) =>
                    setError(message(error)),
                  )
                }
              >
                <FolderOpen size={16} />
                {de.revealExport}
              </button>
            )}
            <button
              type="button"
              className="button"
              onClick={() =>
                void downloadExport(result.id).catch((e) =>
                  setError(message(e)),
                )
              }
            >
              <Download size={16} />
              {de.download}
            </button>
          </div>
        ) : (
          <FormActions
            onClose={onClose}
            busy={busy}
            label={de.export}
            disabled={scopeLoading || !!scopeError || (!!seriesId && !value.asset_ids.length)}
            cancelLabel={busy ? t("Schließen (Export läuft weiter)") : de.cancel}
          />
        )}
      </form>
    </Modal>
  );
}
