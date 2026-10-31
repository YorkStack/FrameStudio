import { styleName } from "./i18n";
import { formatText } from "./i18n";
import { localizedRecord } from "./i18n";
import { t } from "./i18n";
import { useEffect, useRef, useState } from "react";
import {
  ArrowUpRight,
  ChevronDown,
  Layers3,
  Plus,
  Sparkles,
  Trash2,
  X,
  ListChecks,
  Info,
  Upload,
} from "lucide-react";
import { rpc, message, money, fileBase64 } from "./api";
import { de } from "./i18n";
import { AssetImage, ErrorBox, Field, Modal } from "./components";
import PromptAssistant from "./PromptAssistant";
import type {
  AppState,
  Asset,
  Draft,
  Project,
  RecordData,
  Model,
  Variant,
} from "./types";
const blocks = [
  "goal",
  "scene",
  "character",
  "view",
  "pose",
  "expression",
  "gaze",
  "mouth",
  "composition",
  "output",
  "avoid",
] as const;
const blockLabels = localizedRecord({
  goal: de.usage,
  scene: de.scene,
  character: de.promptCharacterNotes,
  view: de.view,
  pose: de.pose,
  expression: de.expression,
  gaze: de.gaze,
  mouth: de.mouth,
  composition: de.composition,
  output: de.output,
  avoid: de.avoid,
});
const qualityLabels: Record<string, string> = localizedRecord({
  low: de.qualityLow,
  medium: de.qualityMedium,
  high: de.qualityHigh,
  xhigh: de.qualityXhigh,
  max: de.qualityMax,
});
const choices = (): Record<keyof Variant, string[]> => ({
  view: [t("frontal"), t("Profil"), t("Dreiviertel"), t("Rücken")],
  pose: [t("stehen"), t("sitzen"), t("gehen"), t("zeigen"), t("winken")],
  expression: [t("neutral"), t("freundlich"), t("traurig"), t("überrascht"), t("verärgert")],
  mouth: [
    t("geschlossen"),
    t("offen"),
    "A",
    "E",
    "I",
    "O",
    "U",
    "M / B / P",
    "F / V",
  ],
});
export function freshDraft(project?: Project, model?: string): Draft {
  return {
    description: project?.prompt_template?.instruction || "",
    style_id: project?.style_id || "",
    character_id: "",
    reference_ids: [],
    provider: "test",
    model: model || "deterministic-v1",
    aspect_ratio: project?.aspect_ratio || "1:1",
    resolution: "1K",
    count: 1,
    blocks: {...Object.fromEntries(blocks.map((b) => [b, ""])), scene:project?.prompt_template?.scene || "", character:project?.prompt_template?.character || ""},
    variants: [],
  };
}
export default function Generator({
  state,
  project,
  assets,
  draft,
  setDraft,
  onSubmitted,
  onMode,
  onReferencesImported,
  onEditCharacter,
  onEditProject,
  onTemplateSaved,
  previewMode = false,
  quickMode = false,
}: {
  state: AppState;
  project: Project;
  assets: Asset[];
  draft: Draft;
  setDraft: (draft: Draft) => void;
  onSubmitted: (result?: RecordData) => Promise<void>;
  onMode: (mode: string) => void;
  onEditCharacter?: (character?: RecordData) => void;
  onEditProject?: () => void;
  onTemplateSaved?: () => Promise<void>;
  previewMode?: boolean;
  quickMode?: boolean;
  onReferencesImported?: () => Promise<void>;
}) {
  const [preview, setPreview] = useState("");
  const [estimate, setEstimate] = useState<RecordData | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [series, setSeries] = useState(false);
  const [assistant, setAssistant] = useState<"scene"|"character"|"instruction"|"all"|null>(null);
  const [templateNotice, setTemplateNotice] = useState("");
  const [references, setReferences] = useState(false);
  const [importingReferences, setImportingReferences] = useState(false);
  const [referenceError, setReferenceError] = useState("");
  const [importedReferences, setImportedReferences] = useState<Asset[]>([]);
  const referenceInput = useRef<HTMLInputElement>(null);
  const sceneSection = useRef<HTMLFieldSetElement>(null);
  const characterSection = useRef<HTMLFieldSetElement>(null);
  const instructionSection = useRef<HTMLFieldSetElement>(null);
  const jumpTo = (ref: {current:HTMLFieldSetElement | null}) => { ref.current?.scrollIntoView({block:"start"}); ref.current?.focus({preventScroll:true}); };
  const importBusy = useRef(false);
  const owner = useRef(project.id);
  owner.current = project.id;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { setImportedReferences([]); setReferenceError(""); setReferences(false); }, [project.id]);
  const referenceAssets = [...importedReferences, ...assets.filter((asset) => !importedReferences.some((entry) => entry.id === asset.id))];
  const [unknown, setUnknown] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [parameterNotice, setParameterNotice] = useState("");
  const idempotency = useRef(crypto.randomUUID());
  const expert = !quickMode && state.settings.mode === "expert";
  const models = state.models || [];
  const keyReady = (provider: string) => Boolean(state.key_status?.providers?.[provider]?.configured);
  const configuredProvider = ["gemini", "openai"].find(keyReady);
  const providerEnabled = draft.provider === "test" ? !configuredProvider : keyReady(draft.provider);
  // A new draft starts locally; prefer a configured image provider once status is known.
  useEffect(() => {
    if (draft.provider === "test" && configuredProvider) {
      changeModel(models.find((entry) => entry.provider === configuredProvider));
    }
  }, [configuredProvider, draft, models]);
  const model = models.find((m) => m.id === draft.model);
  const selectedStyle = state.styles.find((style) => style.id === draft.style_id);
  const characters = state.characters.filter(
    (c) => c.project_id === project.id,
  );
  const selectedCharacter = characters.find(c => c.id === draft.character_id);
  const blockField = (key: keyof typeof blockLabels, rows = 2) => <Field key={key} label={blockLabels[key]}><textarea rows={rows} maxLength={8000} value={draft.blocks[key] || ""} onChange={event => set("blocks", {...draft.blocks, [key]:event.target.value})} /></Field>;
  const set = (key: keyof Draft, value: any) =>
    setDraft({ ...draft, [key]: value });
  const payload = {
    ...draft,
    project_id: project.id,
    variants: draft.variants.length ? draft.variants : undefined,
    quality: model?.qualities?.length
      ? draft.quality || model.default_quality || "medium"
      : undefined,
    style_id: draft.style_id || undefined,
    character_id: draft.character_id || undefined,
    count: draft.variants.length ? 1 : draft.count,
  };
  const serialized = JSON.stringify(payload);
  const promptContext = JSON.stringify([project.description, selectedStyle, selectedCharacter]);
  useEffect(() => {
    if (!JSON.parse(serialized).description.trim()) {
      setError("");
      setPreview("");
      setEstimate(null);
      setChecking(false);
      setSubmitted(false);
      return;
    }
    let live = true;
    idempotency.current = crypto.randomUUID();
    setSubmitted(false);
    setEstimate(null);
    setUnknown(false);
    setChecking(true);
    const timer = setTimeout(async () => {
      try {
        const args = JSON.parse(serialized);
        const [p, c] = await Promise.all([
          rpc("prompt.preview", args),
          rpc("cost.estimate", args),
        ]);
        if (live) {
          setPreview(p.prompt);
          setEstimate(c);
          setError("");
        }
      } catch (e) {
        if (live) {
          setError(message(e));
          setPreview("");
        }
      } finally {
        if (live) setChecking(false);
      }
    }, 350);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [serialized, promptContext, state.pricing_revision]);
  async function submit() {
    if (busy || submitted || !providerEnabled) return;
    setBusy(true);
    setError("");
    try {
      // Explicit start action checks local readability, without a provider request.
      if (draft.provider !== "test") {
        try { await rpc("secret.check", { provider: draft.provider }); }
        catch (cause) { throw new Error(`${de.keyPreflightFailed} ${message(cause)}`); }
      }
      const result = await rpc("job.submit", {
        ...payload,
        idempotency_key: idempotency.current,
        expected_price_version: estimate?.price_version,
        unknown_price_approved: unknown,
      });
      setSubmitted(true);
      await onSubmitted(result);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function importReferencePhotos(files: File[]) {
    if (!files.length || importBusy.current) return;
    const limit = Number(model?.max_references || 8);
    if (draft.reference_ids.length + files.length > limit) {
      setReferenceError(`${de.referenceLimit} ${limit}`);
      return;
    }
    importBusy.current = true;
    setImportingReferences(true);
    setReferenceError("");
    const projectId = project.id;
    const selected = [...draft.reference_ids];
    try {
      for (const file of files) {
        const asset = await rpc<Asset>("asset.import", { project_id: projectId, name: file.name, description: file.name.replace(/\.[^.]+$/, ""), data_base64: await fileBase64(file) });
        selected.push(asset.id);
        if (mounted.current && owner.current === projectId) {
          setImportedReferences((current) => [...current, asset]);
          setDraft({ ...draft, reference_ids: [...selected] });
        }
      }
    } catch (cause) {
      if (mounted.current && owner.current === projectId) setReferenceError(message(cause));
    } finally {
      if (mounted.current && owner.current === projectId) {
        try { await onReferencesImported?.(); }
        catch (cause) { setReferenceError(message(cause)); }
      }
      importBusy.current = false;
      if (mounted.current) setImportingReferences(false);
    }
  }
  function changeModel(next: Model | undefined) {
    if (!next) return;
    const resolution = next.resolutions?.includes(draft.resolution)
      ? draft.resolution
      : next.resolutions?.[0] || "";
    const aspect_ratio = next.aspect_ratios?.includes(draft.aspect_ratio)
      ? draft.aspect_ratio
      : next.aspect_ratios?.[0] || "";
    const quality = next.qualities?.length
      ? next.qualities.includes(draft.quality || "")
        ? draft.quality
        : next.default_quality || "medium"
      : undefined;
    const changes = [
      resolution !== draft.resolution ? `${de.resolution}: ${resolution}` : "",
      aspect_ratio !== draft.aspect_ratio ? `${de.aspect}: ${aspect_ratio}` : "",
      draft.quality && quality !== draft.quality
        ? `${de.quality}: ${quality ? qualityLabels[quality] || quality : de.parameterUnavailable}`
        : "",
    ].filter(Boolean);
    setParameterNotice(changes.length ? changes.join(" · ") : "");
    setDraft({
      ...draft,
      provider: next.provider,
      model: next.id,
      resolution,
      aspect_ratio,
      quality,
    });
  }
  return (
    <aside className={`generator ${quickMode ? "quick-generator" : ""}`} aria-label={t("Generierung")}>
      <div className="panel-heading">
        <div>
          <span className="eyebrow">{t("DEIN NÄCHSTES BILD")}</span>
          <h2>{quickMode ? de.singleImage : t("Ideen in Form bringen")}</h2>
        </div>
        <Sparkles size={19} />
      </div>
      {quickMode && <p className="hint quick-intro">{de.singleImageHint}</p>}
      <div hidden={quickMode} className="mode-switch" aria-label={t("Bedienmodus")}>
        <button
          className={!expert ? "active" : ""}
          onClick={() => onMode("simple")}
        >
          {de.simple}
        </button>
        <button
          className={expert ? "active" : ""}
          onClick={() => onMode("expert")}
        >
          {de.expert}
        </button>
      </div>
      <nav hidden={quickMode} className="prompt-navigation" aria-label={de.promptNavigate}>
        <button onClick={()=>jumpTo(sceneSection)}>{de.scene}</button>
        {!previewMode && <button onClick={()=>jumpTo(characterSection)}>{de.character}</button>}
        <button onClick={()=>jumpTo(instructionSection)}>{de.promptInstruction}</button>
      </nav>
      {!previewMode && !quickMode && <div className="prompt-tools"><button className="text-button" onClick={()=>setAssistant("all")}>{de.promptAIAll}</button><button className="text-button" onClick={async()=>{try {await rpc("project.prompt.save",{id:project.id,prompt_template:{scene:draft.blocks.scene||"",character:draft.blocks.character||"",instruction:draft.description}});await onTemplateSaved?.();setTemplateNotice(de.promptSaved);} catch(e){setTemplateNotice(message(e));}}}>{de.promptSave}</button>{templateNotice && <small role="status">{templateNotice}</small>}</div>}
      <div className="generator-body">
        <fieldset className="prompt-section" ref={sceneSection} tabIndex={-1}>
          <legend hidden={quickMode}>{de.promptSceneSection}</legend><div hidden={quickMode}>
          <p className="hint">{de.promptSceneHint}</p>
          {project.description && <details className="prompt-source"><summary>{de.promptProjectContext}</summary><p>{project.description}</p>{onEditProject && <button className="text-button" onClick={onEditProject}>{de.promptProjectEdit}</button>}</details>}
          <Field label={de.promptSceneText}><textarea rows={3} maxLength={8000} value={draft.blocks.scene || ""} placeholder={de.promptScenePlaceholder} onChange={event => set("blocks", {...draft.blocks,scene:event.target.value})} /></Field>
          <button className="text-button" onClick={()=>setAssistant("scene")}>{de.promptAIScene}</button>
          <details className="prompt-options"><summary>{de.promptSceneDetails}</summary>{(["goal","composition"] as const).map(k=>blockField(k))}</details>
        </div><Field label={de.style}>
          <select
            value={draft.style_id}
            disabled={previewMode}
            onChange={(e) => set("style_id", e.target.value)}
          >
            <option value="">{de.none}</option>
            {state.styles.map((s) => (
              <option key={s.id} value={s.id}>
                {styleName(s)}
              </option>
            ))}
          </select>
        </Field>
        {!previewMode && selectedStyle?.preview_asset_id && <div className="generator-style-preview">
          <AssetImage id={selectedStyle.preview_asset_id} alt={`${de.stylePreviewTitle}: ${selectedStyle.name}`} />
          <div>
            <strong>{de.stylePreviewTitle}</strong>
            <span>{de.stylePreviewVersion} {selectedStyle.preview_style_version || "—"}</span>
            {selectedStyle.preview_provider === "test" && <span>{de.stylePreviewTest}</span>}
            {selectedStyle.preview_style_version !== selectedStyle.version && <span className="stale">{de.stylePreviewStale}</span>}
          </div>
        </div>}
        </fieldset>
        {!previewMode && <fieldset className="prompt-section" ref={characterSection} tabIndex={-1}>
        <legend hidden={quickMode}>{de.promptCharacterSection}</legend><div hidden={quickMode}>
        <p className="hint">{de.promptCharacterHint}</p>
        <Field label={de.character}>
          <select
            value={draft.character_id}
            onChange={(e) => set("character_id", e.target.value)}
          >
            <option value="">{de.none}</option>
            {characters.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
                {c.approved ? " · "+t("freigegeben") : ""}
              </option>
            ))}
          </select>
        </Field>
        {selectedCharacter && <details className="prompt-source">
          <summary>{de.promptCharacterDefinition}</summary>
          <dl>{([['description',de.description],['features',de.features],['clothing',de.clothing],['colors',de.colors]] as const).map(([key,label]) => selectedCharacter[key] ? <div key={key}><dt>{label}</dt><dd>{selectedCharacter[key]}</dd></div> : null)}</dl>
          {selectedCharacter.binding?.length > 0 && <p><strong>{de.binding}: </strong>{selectedCharacter.binding.join(" · ")}</p>}
          {selectedCharacter.reference_ids?.length > 0 && <><p className="hint">{selectedCharacter.approved ? de.promptCharacterReferencesActive : de.promptCharacterReferencesInactive}</p><div className="prompt-character-references">{selectedCharacter.reference_ids.map((id:string)=><AssetImage key={id} id={id} alt={de.character} />)}</div></>}
        </details>}
        {onEditCharacter && <button className="text-button" onClick={()=>onEditCharacter(selectedCharacter)}>{selectedCharacter ? de.promptCharacterEdit : de.newCharacter}</button>}
        <Field label={de.promptCharacterNotes} hint={de.promptCharacterNotesHint}><textarea rows={3} maxLength={8000} value={draft.blocks.character || ""} placeholder={de.promptCharacterPlaceholder} onChange={event=>set("blocks",{...draft.blocks,character:event.target.value})} /></Field>
        <button className="text-button" onClick={()=>setAssistant("character")}>{de.promptAICharacter}</button>
        </div><p className="hint">{de.promptImageReferencesHint}</p>
        <div className="reference-strip">
          {draft.reference_ids.map((id) => (
            <div className="reference-chip" key={id}>
              <AssetImage id={id} />
              <button
                aria-label={de.remove}
                onClick={() =>
                  set(
                    "reference_ids",
                    draft.reference_ids.filter((x) => x !== id),
                  )
                }
              >
                <X size={12} />
              </button>
            </div>
          ))}
          <button className="add-reference" onClick={() => { setReferenceError(""); setReferences(true); }}>
            <Plus size={17} />
            <span>{de.references}</span>
          </button>
        </div>
        </fieldset>}
        <fieldset className="prompt-section" ref={instructionSection} tabIndex={-1}>
          <legend hidden={quickMode}>{de.promptInstructionSection}</legend>
        <Field label={quickMode ? de.description : de.promptInstruction} hint={quickMode ? undefined : de.promptInstructionHint}>
          <textarea
            className="idea-input"
            rows={3}
            placeholder={quickMode ? de.singleImagePlaceholder : de.promptInstructionPlaceholder}
            maxLength={12000}
            value={draft.description}
            onChange={(e) => set("description", e.target.value)}
          />
        </Field>
        <button className="button ghost small assistant-open" onClick={() => setAssistant("instruction")}><Sparkles size={14} />{de.assistantOpen}</button>
          <div hidden={quickMode}><p className="hint">{de.promptAssistantHint}</p>
          <details className="prompt-options"><summary>{de.promptPoseDetails}</summary>{(["view","pose","expression","gaze","mouth"] as const).map(k=>blockField(k))}</details>
          <details className="prompt-options"><summary>{de.promptOutputDetails}</summary>{(["output","avoid"] as const).map(k=>blockField(k))}</details>
        </div></fieldset>
        <div className="field-row">
          <Field label={de.aspect}>
            <select
              value={draft.aspect_ratio}
              onChange={(e) => set("aspect_ratio", e.target.value)}
            >
              {(
                model?.aspect_ratios || [
                  "1:1",
                  "16:9",
                  "9:16",
                  "4:3",
                  "3:4",
                  "3:2",
                  "2:3",
                ]
              ).map((r: string) => (
                <option key={r}>{r}</option>
              ))}
            </select>
          </Field>
          <Field label={de.imageCount}>
            <input
              type="number"
              min={1}
              max={8}
              disabled={previewMode || quickMode || draft.variants.length > 0}
              value={draft.count}
              onChange={(e) =>
                set("count", Math.max(1, Math.min(8, Number(e.target.value))))
              }
            />
          </Field>
        </div>
        <Field label={de.provider}>
          <select
            value={draft.provider}
            onChange={(e) =>
              changeModel(models.find((m) => m.provider === e.target.value))
            }
          >
            {!configuredProvider && <option value="test">{de.test}</option>}
            {[{ id: "gemini", label: de.gemini }, { id: "openai", label: de.openai }].map(({ id, label }) =>
              <option key={id} value={id} disabled={!keyReady(id)}>{label}{!keyReady(id) ? ` · ${state.key_status?.providers?.[id]?.status === "unavailable" ? de.keyUnavailable : de.keyMissing}` : ""}</option>
            )}
          </select>
        </Field>
        {!providerEnabled && <p className="warning">{de.providerNeedsKey}</p>}
        {parameterNotice && (
          <p className="hint" role="status">
            {de.modelParametersChanged} {parameterNotice}
          </p>
        )}
        {!expert && model?.qualities?.length && (
          <p className="hint">
            {model.name || model.id} · {de.quality}: {qualityLabels[payload.quality || "medium"] || payload.quality}
          </p>
        )}
        {expert && (
          <>
            <Field label={de.model}>
              <select
                value={draft.model}
                onChange={(e) =>
                  changeModel(models.find((m) => m.id === e.target.value))
                }
              >
                {models
                  .filter((m) => m.provider === draft.provider)
                  .map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name || m.id}
                    </option>
                  ))}
              </select>
            </Field>
            <Field label={de.resolution}>
              <select
                value={draft.resolution}
                onChange={(e) => set("resolution", e.target.value)}
              >
                {(model?.resolutions || ["1K", "2K", "4K"]).map((r: string) => (
                  <option key={r}>{r}</option>
                ))}
              </select>
            </Field>
            {model?.qualities?.length && (
              <Field label={de.quality}>
                <select
                  value={payload.quality}
                  onChange={(e) => set("quality", e.target.value)}
                >
                  {model.qualities.map((quality) => (
                    <option key={quality} value={quality}>
                      {qualityLabels[quality] || quality}
                    </option>
                  ))}
                </select>
              </Field>
            )}

          </>
        )}
        {!previewMode && !quickMode && <>
        <button className="series-button" onClick={() => setSeries(true)}>
          <Layers3 size={18} />
          <span>
            {draft.variants.length
              ? formatText("{0} Varianten geplant",draft.variants.length)
              : de.series}
          </span>
          <ArrowUpRight size={15} />
        </button>
        {draft.variants.length > 0 && (
          <button className="text-button" onClick={() => set("variants", [])}>{t("Serienplanung entfernen")}</button>
        )}
        </>}
        <details className="prompt-preview" open={!quickMode}>
          <summary>
            {de.prompt}
            <ChevronDown size={14} />
          </summary>
          <pre>
            {checking ? de.loading : preview || t("Beschreibe zuerst dein Bild.")}
          </pre>
          <p className="hint">{de.immutable}</p>
        </details>
        {draft.provider === "test" && (
          <p className="notice">
            <Info size={15} />
            {de.testNotice}
          </p>
        )}
        {draft.character_id && <p className="hint">{de.consistency}</p>}
      </div>
      <div className="generation-footer">
        <ErrorBox error={error} />
        {estimate?.warnings?.map((w: string, i: number) => (
          <p className="warning" key={i}>
            {t(w)}
          </p>
        ))}
        {estimate && estimate.total_microusd == null && !checking && (
          <label className="check-field">
            <input
              type="checkbox"
              checked={unknown}
              onChange={(e) => setUnknown(e.target.checked)}
            />
            {de.approveUnknown}
          </label>
        )}
        <div className="estimate">
          <div>
            <span>{de.estimated}</span>
            <strong>
              {checking || !estimate ? "—" : money(estimate.total_microusd)}
            </strong>
          </div>
          <small>
            {draft.variants.length || draft.count}{" "}
            {(draft.variants.length || draft.count) === 1 ? t("Bild") : t("Bilder")}
            {estimate && (
              <>
                {" "}
                · {money(estimate.per_image_microusd)} {de.estimatedEach}
              </>
            )}
          </small>
        </div>
        <button
          className="generate-button"
          disabled={
            busy ||
            !providerEnabled ||
            submitted ||
            checking ||
            !draft.description.trim() ||
            !estimate ||
            (estimate.total_microusd == null && !unknown)
          }
          onClick={() => void submit()}
        >
          <Sparkles size={18} />
          {busy
            ? de.busy
            : submitted
              ? de.requestSent
              : formatText("{0} {1} erzeugen", draft.variants.length || draft.count, (draft.variants.length || draft.count) === 1 ? de.image : de.seriesImages)}
          <span aria-hidden="true">↗</span>
        </button>
        <p className="footer-note">{de.apiPrivacy}</p>
      </div>
      {assistant && <PromptAssistant key={assistant} projectId={project.id} text={assistant === "scene" ? draft.blocks.scene || "" : assistant === "character" ? draft.blocks.character || "" : draft.description} target={assistant} sections={{scene:draft.blocks.scene||"",character:draft.blocks.character||"",instruction:draft.description}} pricingRevision={state.pricing_revision} onClose={() => setAssistant(null)} onApply={(text, fields) => { if(fields) setDraft({...draft,description:fields.instruction,blocks:{...draft.blocks,scene:fields.scene,character:fields.character}}); else if(assistant === "instruction") set("description",text); else set("blocks",{...draft.blocks,[assistant]:text}); setAssistant(null); }} />}
      {series && (
        <SeriesEditor
          variants={draft.variants}
          onSave={(variants) => {
            setDraft({ ...draft, variants, count: 1 });
            setSeries(false);
          }}
          onClose={() => setSeries(false)}
        />
      )}
      {references && (
        <Modal title={de.referenceChoose} onClose={() => { if (!importingReferences) setReferences(false); }}>
          <p className="hint">{t("Nur ausgewählte Referenzen werden für diesen Auftrag verwendet.")}</p>
          <input ref={referenceInput} type="file" hidden multiple accept="image/png,image/jpeg,image/webp,image/tiff" aria-label={de.referenceUpload} onChange={(event) => { const files = Array.from(event.currentTarget.files || []); event.currentTarget.value = ""; void importReferencePhotos(files); }} />
          <button className="button" disabled={importingReferences} onClick={() => referenceInput.current?.click()}><Upload size={16} />{importingReferences ? de.busy : de.referenceUpload}</button>
          <p className="hint">{de.referenceUploadHint}</p>
          <ErrorBox error={referenceError} />
          <p className="hint" role="status">{importingReferences ? de.referenceImporting : `${draft.reference_ids.length} ${de.referenceSelected}`}</p>
          {referenceAssets.length ? (
            <div className="reference-grid">
              {referenceAssets.map((a) => (
                <button
                  className={`reference-option ${draft.reference_ids.includes(a.id) ? "selected" : ""}`}
                  disabled={importingReferences || (!draft.reference_ids.includes(a.id) && draft.reference_ids.length >= Number(model?.max_references || 8))}
                  aria-label={a.description || a.id.slice(0, 8)}
                  aria-pressed={draft.reference_ids.includes(a.id)}
                  key={a.id}
                  onClick={() =>
                    set(
                      "reference_ids",
                      draft.reference_ids.includes(a.id)
                        ? draft.reference_ids.filter((id) => id !== a.id)
                        : [...draft.reference_ids, a.id],
                    )
                  }
                >
                  <AssetImage id={a.id} alt={a.description} />
                  <span>{a.description}</span>
                </button>
              ))}
            </div>
          ) : (
            <p>{de.noReference}</p>
          )}
          <div className="form-actions">
            <button
              className="button primary"
              disabled={importingReferences}
              onClick={() => setReferences(false)}
            >
              {de.close}
            </button>
          </div>
        </Modal>
      )}
    </aside>
  );
}
function SeriesEditor({
  variants,
  onSave,
  onClose,
}: {
  variants: Variant[];
  onSave: (value: Variant[]) => void;
  onClose: () => void;
}) {
  const [list, setList] = useState<Variant[]>(
    variants.length
      ? variants
      : [
          {
            view: t("frontal"),
            pose: t("stehen"),
            expression: t("neutral"),
            mouth: t("geschlossen"),
          },
        ],
  );
  const [next, setNext] = useState<Variant>({
    view: t("Dreiviertel"),
    pose: t("winken"),
    expression: t("freundlich"),
    mouth: t("geschlossen"),
  });
  return (
    <Modal title={de.series} onClose={onClose} wide>
      <p className="hint">{t("Stelle jedes Bild einzeln zusammen. Maximal 32 Varianten; es wird kein kartesisches Produkt erzeugt.")}</p>
      <div className="variant-builder">
        {(Object.keys(choices()) as (keyof Variant)[]).map((key) => (
          <Field label={de[key]} key={key}>
            <input
              list={`choices-${key}`}
              value={next[key]}
              onChange={(e) => setNext({ ...next, [key]: e.target.value })}
            />
            <datalist id={`choices-${key}`}>
              {choices()[key].map((v) => (
                <option key={v}>{v}</option>
              ))}
            </datalist>
          </Field>
        ))}
        <button
          className="button"
          disabled={list.length >= 32}
          onClick={() => setList([...list, { ...next }])}
        >
          <Plus size={16} />
          {de.addVariant}
        </button>
      </div>
      <h3>
        <ListChecks size={18} /> {de.planned} · {list.length}
      </h3>
      <div className="variant-list">
        {list.map((v, index) => (
          <div key={index} className="variant-row">
            <span>{String(index + 1).padStart(2, "0")}</span>
            <p>
              {v.view} · {v.pose} · {v.expression} · {v.mouth}
            </p>
            <button
              className="icon-button"
              aria-label={`Variante ${index + 1} entfernen`}
              onClick={() => setList(list.filter((_, i) => i !== index))}
            >
              <Trash2 size={15} />
            </button>
          </div>
        ))}
      </div>
      <p className="hint">
        {de.consistency}{t("Die Gesamtkosten erscheinen nach Übernahme rechts vor dem Start.")}</p>
      <div className="form-actions">
        <button className="button ghost" onClick={onClose}>
          {de.cancel}
        </button>
        <button
          className="button primary"
          onClick={() => onSave(list)}
          disabled={!list.length}
        >{t("Liste übernehmen")}</button>
      </div>
    </Modal>
  );
}
