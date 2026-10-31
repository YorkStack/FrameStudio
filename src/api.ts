import { translateError } from "./i18n";
import { locale } from "./i18n";
import { t } from "./i18n";
import { invoke } from "@tauri-apps/api/core";
import type { RecordData } from "./types";
declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __FRAMESTUDIO_TOKEN__?: string;
  }
}
export const isDesktop = () => Boolean(window.__TAURI_INTERNALS__);
export async function rpc<T = any>(
  op: string,
  args: RecordData = {},
): Promise<T> {
  if (isDesktop()) return invoke<T>("rpc", { op, args });
  const response = await fetch("/rpc", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(window.__FRAMESTUDIO_TOKEN__
        ? { Authorization: `Bearer ${window.__FRAMESTUDIO_TOKEN__}` }
        : {}),
    },
    body: JSON.stringify({ op, args }),
  });
  const result = await response.json();
  if (!response.ok || result.error)
    throw new Error(
      typeof result.error === "string"
        ? result.error
        : `Lokaler Dienst: ${response.status}`,
    );
  return result;
}
export function money(value: number | null | undefined): string {
  return value == null
    ? t("Unbekannt")
    : new Intl.NumberFormat(locale(), {
        style: "currency",
        currency: "USD",
        minimumFractionDigits: value !== 0 && value < 10000 ? 4 : 2,
        maximumFractionDigits: 4,
      }).format(value / 1_000_000);
}
export function message(error: unknown): string {
  return translateError(error instanceof Error ? error.message : String(error));
}
export function fileBase64(file: File): Promise<string> {
  if (file.size > 64 * 1024 * 1024)
    return Promise.reject(new Error(t("Die Datei ist größer als 64 MB.")));
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",")[1]);
    reader.onerror = () =>
      reject(new Error(t("Datei konnte nicht gelesen werden.")));
    reader.readAsDataURL(file);
  });
}
export async function downloadExport(id: string) {
  const file = await rpc("export.read", { id });
  const bytes = Uint8Array.from(atob(file.data_base64), (c) => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: file.mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download =
    file.name ||
    `FrameStudio-${id}.${file.mime === "application/zip" ? "zip" : "bin"}`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function downloadJson(value:unknown,name:string) { const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:"application/json"}));const a=document.createElement("a");a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000); }

export async function chooseDirectory(): Promise<string|null> { return isDesktop() ? invoke("choose_directory") : null; }
