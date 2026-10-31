import { translateError } from "./i18n";
import { locale } from "./i18n";
import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { rpc, money, message } from "./api";
import { ErrorBox } from "./components";
import { de } from "./i18n";
import type { RecordData } from "./types";

const sources: Record<string, string> = {
  gemini: "https://ai.google.dev/gemini-api/docs/pricing",
  openai: "https://developers.openai.com/api/docs/pricing",
  anthropic: "https://platform.claude.com/docs/en/about-claude/pricing.md",
};
const providerNames: Record<string, string> = {
  gemini: de.gemini,
  openai: de.openai,
  anthropic: de.anthropic,
};
function checkedDate(value: unknown) {
  if (typeof value !== "string") return de.unknown;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return de.unknown;
  return /^\d{4}-\d{2}-\d{2}$/.test(value) ? date.toLocaleDateString(locale(), { dateStyle: "medium", timeZone: "UTC" }) : date.toLocaleString(locale(), { dateStyle: "medium", timeStyle: "short" });
}
function Rates({ price }: { price: RecordData }) {
  const rates = [
    [de.pricingInput, price.input_microusd_per_million],
    [de.pricingTextInput, price.text_input_microusd_per_million],
    [de.pricingImageInput, price.image_input_microusd_per_million],
    [de.pricingTextOutput, price.text_output_microusd_per_million],
    [de.pricingImageOutput, price.image_output_microusd_per_million],
  ].filter(([, value]) => value != null);
  return <dl className="pricing-rates">{rates.map(([label, value]) => <div key={String(label)}><dt>{label}</dt><dd>{money(value as number)}</dd></div>)}</dl>;
}

export default function PricingPanel({ onUpdated, expanded = false }: { onUpdated: () => Promise<void> | void; expanded?: boolean }) {
  const [catalog, setCatalog] = useState<RecordData | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [copied, setCopied] = useState("");
  useEffect(() => {
    let live = true;
    rpc("pricing.catalog").then((result) => { if (live) setCatalog(result); }).catch((cause) => { if (live) setError(message(cause)); });
    return () => { live = false; };
  }, []);
  async function refresh() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await rpc("pricing.refresh");
      setCatalog(result);
      await onUpdated();
      setNotice(result.providers.some((provider: RecordData) => provider.status === "error") ? de.pricingPartial : de.pricingUpdated);
    } catch (cause) {
      setError(message(cause));
    } finally {
      setBusy(false);
    }
  }
  async function copySource(provider: string) {
    try {
      await navigator.clipboard.writeText(sources[provider]);
      setCopied(provider);
      setError("");
    } catch { setError(de.pricingCopyFailed); }
  }
  return <details className="pricing-panel" open={expanded}>
    <summary>{de.pricingTitle}</summary>
    <div className="pricing-toolbar"><p>{de.pricingIntro}</p><button type="button" className="button small" disabled={busy} onClick={() => void refresh()}><RefreshCw size={15} className={busy ? "spin" : ""} />{busy ? de.pricingRefreshing : de.pricingRefresh}</button></div>
    <ErrorBox error={error} />
    {notice && <p className="hint" role="status">{notice}</p>}
    {!catalog && !error && <p className="hint">{de.loading}</p>}
    <div className="pricing-providers">{catalog?.providers?.map((provider: RecordData) => <section className="pricing-provider" key={provider.id} aria-label={`${de.pricingTitle}: ${providerNames[provider.id] || provider.name}`}>
      <div className="pricing-provider-heading"><h3>{providerNames[provider.id] || provider.name}</h3><span className={provider.status === "error" ? "warning" : "hint"}>{provider.status === "updated" ? de.pricingLive : provider.status === "error" ? de.pricingRetained : de.pricingBundled}</span></div>
      <p className="hint">{de.pricingChecked}: {checkedDate(provider.checked_at)}</p>
      {provider.last_attempt_at && provider.status === "error" && <p className="hint">{de.pricingAttempt}: {checkedDate(provider.last_attempt_at)}</p>}
      {provider.id === "anthropic" && <p className="pricing-capability">{de.claudeNoImages}</p>}
      {provider.error && <p className="warning">{translateError(provider.error)}</p>}
      {provider.models?.map((model: RecordData) => <div className="pricing-model" key={model.id}>
        <h4>{model.name || model.id}</h4>
        {model.examples?.length > 0 && <><p className="hint">{de.pricingOutputOnly}</p><table><thead><tr><th>{de.pricingExample}</th><th>{de.pricingPerImage}</th></tr></thead><tbody>{model.examples.map((example: RecordData, index: number) => <tr key={index}><td>{example.label || [example.resolution, example.aspect_ratio, example.quality].filter(Boolean).join(" · ")}</td><td>{money(example.output_microusd)}</td></tr>)}</tbody></table></>}
        <p className="pricing-rate-label">{de.pricingPerMillion}</p>
        <Rates price={model.price_snapshot || {}} />
        <p className="pricing-version">{model.price_snapshot?.version}</p>
      </div>)}
      <p className="pricing-source"><span>{de.pricingSource}</span><span className="pricing-source-url">{sources[provider.id]}</span><button type="button" className="button small" onClick={() => void copySource(provider.id)}>{copied === provider.id ? de.copied : de.pricingCopy}</button></p>
    </section>)}</div>
    <p className="hint">{de.pricingNotice}</p>
  </details>;
}
