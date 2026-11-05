// Generates the documentation screenshots in docs/images (English) and docs/de/images (German).
//
//   cargo build --manifest-path src-tauri/Cargo.toml --locked
//   npx playwright test -c scripts/docs-screenshots.config.ts
//
// Uses a fresh synthetic workspace, the free deterministic test provider and the bundled
// neutral style previews. Key status is faked as metadata only; no OS credential store is read,
// no provider is contacted and no access token value is ever shown.
import { test, expect, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = fileURLToPath(new URL("..", import.meta.url));
const binary = path.join(repo, "src-tauri/target/debug/framestudio");
const only = process.env.DOCS_SHOTS_ONLY?.split(",");

type Lang = "en" | "de";
const T = {
  en: {
    dir: "docs/images",
    project: "Workshop illustrations",
    brief: "Friendly retro robot scenes for a presentation about the workshop.",
    scene: "A bright workshop with a wooden desk, soft daylight and a potted plant.",
    character: "A friendly retro robot with brass joints and a round blue display.",
    instruction: "Show the robot waving with its right hand, facing the viewer.",
    charName: "Robbie",
    charDesc: "Friendly retro robot used throughout the presentation.",
    features: "Round blue display, brass joints, short antenna",
    clothing: "Small orange scarf",
    colors: "Brass, teal, orange",
    seriesDesc: "Robbie poses for the onboarding slides",
    poses: ["waving", "pointing at a whiteboard", "carrying a toolbox"],
    tokens: [
      ["Layout script", "Design team", "Batch export of slide images", "rest"],
      ["Local agent", "Own laptop", "Prepare character series via MCP", "mcp"],
      ["Old test script", "Testing", "Evaluation in October", "rest"],
    ],
    styleNames: ["Comic with clean outlines", "Ligne claire", "Graphic novel", "Manga/anime", "Explainer cartoon", "Watercolor", "Oil painting", "Photorealistic"],
    settings: "Settings",
    nav: "Settings sections",
    close: "Close",
    libraries: "Libraries",
    styles: "Style library",
    characters: "Characters",
    singleImage: "Create single image",
    description: "Description",
    sceneText: "Scene description",
    instruction: "Individual instruction",
    characterSelect: "Character",
    seriesButton: /Series approvals/,
    export: "Export",
    sections: { appearance: "Appearance", keys: "API keys", storage: "Storage & export", budgets: "Budgets & jobs", pricing: "Pricing & providers", access: "REST, MCP & access" },
    singlePrompt: "A friendly retro robot beside a wooden desk, a small green plant and warm daylight. Clean outlines, soft colors, no lettering.",
  },
  de: {
    dir: "docs/de/images",
    project: "Werkstatt-Illustrationen",
    brief: "Freundliche Retro-Roboter-Szenen für eine Präsentation über die Werkstatt.",
    scene: "Eine helle Werkstatt mit Holzschreibtisch, sanftem Tageslicht und einer Topfpflanze.",
    character: "Ein freundlicher Retro-Roboter mit Messinggelenken und rundem blauem Display.",
    instruction: "Zeige den Roboter, wie er mit der rechten Hand winkt und zum Betrachter schaut.",
    charName: "Robbie",
    charDesc: "Freundlicher Retro-Roboter, der durch die ganze Präsentation führt.",
    features: "Rundes blaues Display, Messinggelenke, kurze Antenne",
    clothing: "Kleiner orangefarbener Schal",
    colors: "Messing, Petrol, Orange",
    seriesDesc: "Robbie-Posen für die Einführungsfolien",
    poses: ["winkend", "zeigt auf ein Whiteboard", "trägt einen Werkzeugkasten"],
    tokens: [
      ["Layout-Skript", "Designteam", "Folienbilder gesammelt exportieren", "rest"],
      ["Lokaler Agent", "Eigener Laptop", "Charakterserien über MCP vorbereiten", "mcp"],
      ["Altes Testskript", "Test", "Erprobung im Oktober", "rest"],
    ],
    styleNames: ["Comic mit klarer Kontur", "Ligne claire", "Graphic Novel", "Manga/Anime", "Cartoon für Erklärfilme", "Aquarell", "Ölgemälde", "Fotorealistisch"],
    settings: "Einstellungen",
    nav: "Einstellungsbereiche",
    close: "Schließen",
    libraries: "Bibliotheken",
    styles: "Stilbibliothek",
    characters: "Charaktere",
    singleImage: "Einzelbild erzeugen",
    description: "Beschreibung",
    sceneText: "Szenenbeschreibung",
    instruction: "Einzelanweisung",
    characterSelect: "Charakter",
    seriesButton: /Serienfreigaben/,
    export: "Exportieren",
    sections: { appearance: "Darstellung", keys: "API-Schlüssel", storage: "Speicher & Export", budgets: "Budgets & Aufträge", pricing: "Preise & Anbieter", access: "REST, MCP & Zugänge" },
    singlePrompt: "Ein freundlicher Retro-Roboter neben einem Holzschreibtisch, eine kleine grüne Pflanze und warmes Tageslicht. Klare Konturen, sanfte Farben, keine Schrift.",
  },
} as const;

function rpc(store: string, op: string, args: Record<string, unknown> = {}) {
  return new Promise<any>((resolve, reject) => {
    const child = spawn(binary, ["--data-dir", store, "--rpc"], { stdio: ["pipe", "pipe", "pipe"] });
    let out = "";
    let err = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    child.on("close", (code) => {
      if (code !== 0) reject(new Error(`${op}: ${err}`));
      else resolve(JSON.parse(out));
    });
    child.stdin.end(JSON.stringify({ op, args }));
  });
}

async function routeCore(page: Page, store: string) {
  await page.route("**/rpc", async (route) => {
    try {
      const req = route.request().postDataJSON();
      if (req.op.startsWith("secret.") || req.op.startsWith("tokens.create")) throw new Error("Disabled for documentation screenshots");
      const result = await rpc(store, req.op, req.args);
      if (req.op === "state")
        result.key_status = {
          providers: Object.fromEntries(
            ["gemini", "openai", "anthropic"].map((p) => [p, { configured: true, status: "configured" }]),
          ),
        };
      if (req.op === "state") result.data_dir = "~/Library/Application Support/com.YorkStack.FrameStudio";
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(result) });
    } catch (e) {
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: String(e) }) });
    }
  });
}

async function seed(store: string, lang: Lang) {
  const t = T[lang];
  await rpc(store, "settings.save", { language: lang, theme: "light", mode: "simple" });
  const styles = await rpc(store, "style.list");
  const style = styles.find((s: any) => s.id === "00000000-0000-4000-8000-000000000003") ?? styles[0];
  const project = await rpc(store, "project.save", { name: t.project, description: t.brief, style_id: style.id, aspect_ratio: "3:2" });
  await rpc(store, "project.prompt.save", {
    id: project.id,
    prompt_template: { scene: t.scene, character: t.character, instruction: t.instruction },
  });
  const previews = JSON.parse(await fs.readFile(path.join(repo, "src/bundled-style-previews.json"), "utf8"));
  const assets = [];
  for (const [i, p] of previews.slice(0, 8).entries()) {
    const bytes = await fs.readFile(path.join(repo, "public", p.file));
    assets.push(
      await rpc(store, "asset.import", {
        project_id: project.id,
        name: path.basename(p.file),
        description: t.styleNames[i],
        data_base64: bytes.toString("base64"),
      }),
    );
  }
  const character = await rpc(store, "character.save", {
    project_id: project.id,
    name: t.charName,
    description: t.charDesc,
    features: t.features,
    clothing: t.clothing,
    colors: t.colors,
    binding: [],
    reference_ids: [assets[0].id],
    approved: true,
  });
  // A pending series with a real price estimate; it is never approved.
  await rpc(store, "series.submit", {
    project_id: project.id,
    description: t.seriesDesc,
    idempotency_key: `docs-series-${lang}`,
    items: t.poses.map((pose) => ({
      description: `${t.charName}: ${pose}`,
      provider: "gemini",
      model: "gemini-nano-banana-2.1",
      character_id: character.id,
      aspect_ratio: "1:1",
      resolution: "1K",
    })),
  });
  // Access inventory: two active entries and one revoked. Token values are discarded.
  const created = [];
  for (const [name, recipient, purpose, scope] of t.tokens)
    created.push((await rpc(store, "tokens.create", { name, recipient, purpose, scope })).access);
  await rpc(store, "tokens.revoke", { id: created[2].id });
  await rpc(store, "settings.save", { rest_enabled: true, mcp_enabled: true });
  return { project, character, assets };
}

async function shot(page: Page, lang: Lang, name: string) {
  if (only && !only.includes(name)) return;
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(repo, T[lang].dir, `${name}.png`) });
}

async function openSettings(page: Page, lang: Lang, section: string) {
  await page.getByRole("button", { name: T[lang].settings, exact: true }).click();
  await page.getByRole("navigation", { name: T[lang].nav }).getByRole("button", { name: section, exact: true }).click();
}

for (const lang of ["en", "de"] as const) {
  test(`documentation screenshots (${lang})`, async ({ page }) => {
    const store = path.join(repo, `.artifacts/docs-store-${lang}`);
    await fs.rm(store, { recursive: true, force: true });
    await fs.mkdir(path.join(repo, T[lang].dir), { recursive: true });
    await seed(store, lang);
    const worker: ChildProcess = spawn(binary, ["--data-dir", store, "--worker"], { stdio: "ignore" });
    try {
      await routeCore(page, store);
      const t = T[lang];
      await page.goto("/");
      await expect(page.getByRole("heading", { name: t.project, exact: true })).toBeVisible();
      // Workspace with a large preview and the three prompt sections filled in.
      await page.getByRole("button", { name: new RegExp(`: ${t.styleNames[4]}$`) }).first().click();
      await page.getByLabel(t.sceneText, { exact: true }).fill(t.scene);
      await page.getByLabel(t.characterSelect, { exact: true }).selectOption({ index: 1 });
      await page.getByLabel(t.instruction, { exact: true }).fill(t.instruction);
      if (lang === "de") await shot(page, lang, "01-workspace");
      await page.getByLabel(t.sceneText, { exact: true }).scrollIntoViewIfNeeded();
      await shot(page, lang, "06-prompt-sections");

      await page.getByRole("navigation", { name: t.libraries }).getByRole("button", { name: new RegExp(t.styles) }).click();
      await shot(page, lang, "07-style-library");

      await page.getByRole("navigation", { name: t.libraries }).getByRole("button", { name: new RegExp(t.characters) }).click();
      await page.getByRole("article").filter({ hasText: t.charName }).getByRole("button").last().click();
      await shot(page, lang, "08-character");
      await page.keyboard.press("Escape");

      await page.getByRole("button", { name: t.project, exact: true }).first().click();
      await page.getByRole("button", { name: t.seriesButton }).click();
      await shot(page, lang, "09-series-approval");
      await page.keyboard.press("Escape");

      for (const [key, name] of [["budgets", "10-budgets"], ["access", "11-access-tokens"], ["storage", "12-storage"], ["pricing", "14-pricing"]] as const) {
        await openSettings(page, lang, t.sections[key]);
        await shot(page, lang, name);
        await page.keyboard.press("Escape");
      }
      if (lang === "de") {
        await openSettings(page, lang, t.sections.appearance);
        await shot(page, lang, "04-language");
        await page.keyboard.press("Escape");
        await openSettings(page, lang, t.sections.keys);
        await shot(page, lang, "03-api-keys");
        await page.keyboard.press("Escape");
        await page.getByRole("button", { name: t.export, exact: true }).first().click();
        await shot(page, lang, "05-export");
        await page.keyboard.press("Escape");
        await page.getByRole("button", { name: t.singleImage }).first().click();
        await page.getByLabel(t.description, { exact: true }).fill(t.singlePrompt);
        await shot(page, lang, "02-single-image");
      }
    } finally {
      worker.kill("SIGTERM");
    }
  });
}
