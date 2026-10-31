import { test, expect, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
const root = path.resolve(".artifacts/ui-store");
const proof = path.resolve(".artifacts/ui-proof");
const binary = path.resolve("src-tauri/target/debug/framestudio");
let worker: ChildProcess;
function call(op: string, args: Record<string, unknown> = {}, store = root, environment: NodeJS.ProcessEnv = {}) {
  return new Promise<any>((resolve, reject) => {
    const child = spawn(binary, ["--data-dir", store, "--rpc"], {
      stdio: ["pipe", "pipe", "pipe"],
      env: { ...process.env, ...environment },
    });
    let out = "";
    let err = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    child.on("close", (code) => {
      if (code !== 0) reject(new Error(err));
      else
        try {
          resolve(JSON.parse(out));
        } catch (e) {
          reject(e);
        }
    });
    child.stdin.end(JSON.stringify({ op, args }));
  });
}
async function routeCore(page: Page, store = root, offlinePrices = false, configuredProviders: string[] = []) {
  await page.route("**/rpc", async (route) => {
    try {
      const req = route.request().postDataJSON();
      if (req.op === "secret.check") throw new Error("No real credential reads in this fixture");
      const result = await call(req.op, req.args, store, offlinePrices && req.op === "pricing.refresh" ? { HTTPS_PROXY: "http://127.0.0.1:1", https_proxy: "http://127.0.0.1:1", ALL_PROXY: "http://127.0.0.1:1", NO_PROXY: "", no_proxy: "" } : {});
      // Explicit metadata-only fixture: UI tests must not select a user's OS keys.
      if (req.op === "state") result.key_status = { providers: Object.fromEntries(["gemini", "openai", "anthropic"].map(provider => [provider, { configured: configuredProviders.includes(provider), status: configuredProviders.includes(provider) ? "configured" : "missing" }])) };
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(result),
      });
    } catch (e) {
      await route.fulfill({
        status: 400,
        contentType: "application/json",
        body: JSON.stringify({ error: String(e) }),
      });
    }
  });
}
test.beforeAll(async () => {
  await fs.rm(root, { recursive: true, force: true });
  await fs.mkdir(proof, { recursive: true });
  await call("settings.save", { rest_enabled: true, theme: "light" });
  worker = spawn(binary, ["--data-dir", root, "--worker"], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  worker.stderr.on("data", () => {});
});
test.afterAll(async () => {
  worker?.kill("SIGTERM");
});
test("actual core: empty, project, style, reference, character, three variants, persist, compare, exports, themes and errors", async ({
  page,
}) => {
  await routeCore(page);
  await page.goto("/");
  await expect(page.getByText("Raum für deine nächste Idee.")).toBeVisible();
  await page.screenshot({ path: path.join(proof, "01-empty-light.png") });
  await page
    .getByRole("button", { name: "Neues Projekt", exact: true })
    .first()
    .click();
  await page.getByLabel("Projektname").fill("Synthetisches Prüfprojekt");
  await page
    .getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })
    .fill("Lokaler Abnahmelauf mit deterministischen Prüfbildern.");
  await page
    .getByLabel("Stil", { exact: true })
    .selectOption({ label: "Ligne claire" });
  await page.getByLabel("Standard-Ausgabeformat").selectOption("webp");
  await page.getByRole("button", { name: "Speichern", exact: true }).click();
  await expect(
    page.getByRole("heading", {
      name: "Synthetisches Prüfprojekt",
      exact: true,
    }),
  ).toBeVisible();
  const state = await call("state");
  const project = state.projects[0];
  const ref = await call("asset.import", {
    project_id: project.id,
    name: "synthetic-reference.png",
    description: "Synthetische Referenz",
    data_base64: await page.evaluate(() => {
      const c = document.createElement("canvas");
      c.width = 128;
      c.height = 128;
      const x = c.getContext("2d")!;
      x.fillStyle = "#d18658";
      x.fillRect(0, 0, 128, 128);
      x.fillStyle = "#fff";
      x.font = "14px sans-serif";
      x.fillText("SYNTHETIC", 20, 70);
      return c.toDataURL("image/png").split(",")[1];
    }),
  });
  await page.reload();
  await expect(
    page.getByRole("button", {
      name: "Bild 1: Synthetische Referenz",
      exact: true,
    }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Charaktere/ }).click();
  await page.getByRole("button", { name: "Neuer Charakter" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Prüffigur Ada");
  await page
    .getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })
    .fill("Synthetische Testfigur, orange Jacke und blaue Brille.");
  await page.getByLabel("Wiedererkennbare Merkmale").fill("Runde Brille");
  await page
    .getByRole("button", { name: "Synthetische Referenz", exact: true })
    .click();
  await page.getByLabel("Referenzen für Generierungen freigegeben").check();
  await page.getByRole("button", { name: "Speichern", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Prüffigur Ada" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Synthetisches Prüfprojekt", exact: true })
    .click();
  await page
    .getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })
    .fill("Ada winkt vor hellem Hintergrund, vollständig sichtbare Figur.");
  await page
    .getByLabel("Charakter", { exact: true })
    .selectOption({ label: "Prüffigur Ada · freigegeben" });
  await page
    .getByRole("button", { name: "Charakterset planen", exact: true })
    .click();
  await page.getByRole("button", { name: "Variante hinzufügen" }).click();
  await page.getByRole("dialog").getByLabel("Ansicht", { exact: true }).fill("Profil");
  await page.getByRole("dialog").getByLabel("Pose", { exact: true }).fill("gehen");
  await page.getByRole("button", { name: "Variante hinzufügen" }).click();
  await expect(
    page.getByRole("heading", { name: "Geplante Bilder · 3" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Liste übernehmen" }).click();
  await expect(
    page.getByRole("button", { name: /3 Bilder erzeugen/ }),
  ).toBeEnabled();
  await page.screenshot({ path: path.join(proof, "02-ready-simple.png") });
  await page.getByRole("button", { name: /3 Bilder erzeugen/ }).click();
  await expect
    .poll(
      async () => {
        const jobs = await call("job.list", { project_id: project.id });
        return jobs.filter((j: any) => j.status === "completed").length;
      },
      { timeout: 30_000 },
    )
    .toBe(3);
  await page
    .getByRole("region", { name: "Aufträge" })
    .getByRole("button", { name: "Schließen" })
    .click();
  await expect(page.locator(".asset-card")).toHaveCount(4);
  await page.screenshot({ path: path.join(proof, "03-gallery-generated.png") });
  await page.getByRole("button", { name: "Vergleichen", exact: true }).click();
  await expect(page.locator(".compare-pane")).toHaveCount(2);
  await page.screenshot({ path: path.join(proof, "04-compare.png") });
  await page.getByRole("button", { name: "Vergleichen", exact: true }).click();
  await page.getByRole("button", { name: "Auswahl aufheben" }).click();
  await page.getByRole("button", { name: "Experte", exact: true }).click();
  await expect(page.getByLabel("Modell", { exact: true })).toBeVisible();
  await page.getByText("Verwendung und Komposition", { exact: true }).click();
  await page
    .getByLabel("Komposition und Ausschnitt")
    .fill(
      "Synthetischer langer Text: " +
        "Alle Figuren vollständig im Bild. ".repeat(50),
    );
  await page.screenshot({ path: path.join(proof, "05-expert-long-text.png") });
  await page
    .getByRole("button", { name: "Einstellungen", exact: true })
    .click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"Darstellung",exact:true}).click();
  await page
    .getByLabel("Erscheinungsbild", { exact: true })
    .selectOption("dark");
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"Speicher & Export",exact:true}).click();
  await page
    .getByLabel("Namensschema", { exact: true })
    .fill("{projekt}_{nummer:04}");
  await page.getByRole("button", { name: "Speichern", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({ path: path.join(proof, "06-dark.png") });
  await page.reload();
  await expect(page.locator(".asset-card")).toHaveCount(4);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: "Exportieren", exact: true }).click();
  await expect(page.getByLabel("Exportformat")).toHaveValue("webp");
  await expect(page.getByLabel("Namensschema", { exact: true })).toHaveValue(
    "{projekt}_{nummer:04}",
  );
  await page.getByLabel("Exportformat").selectOption("png");
  await expect(
    page.getByText("Beispielname: Synthetisches Prüfprojekt_0001.png", {
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Exportieren", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Export erstellt" }),
  ).toBeVisible();
  await page.screenshot({ path: path.join(proof, "07-export.png") });
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Schließen" })
    .click();
  await page
    .getByLabel("Beschreibung oder Tags suchen", { exact: true })
    .fill("absichtlich-keine-treffer");
  await expect(page.getByText("Keine passenden Bilder")).toBeVisible();
  await page.screenshot({ path: path.join(proof, "08-filter-empty.png") });
  await page
    .getByLabel("Beschreibung oder Tags suchen", { exact: true })
    .fill("");
  await page.locator('input[type=file][accept*="image/png"]').setInputFiles({
    name: "invalid-synthetic.png",
    mimeType: "image/png",
    buffer: Buffer.from("clearly not an image"),
  });
  await expect(page.getByRole("alert")).toBeVisible();
  await page.screenshot({
    path: path.join(proof, "09-invalid-import-error.png"),
  });
  const assets = await call("asset.list", {
    project_id: project.id,
    limit: 100,
  });
  expect(assets.total).toBe(4);
  const jobs = await call("job.list", { project_id: project.id });
  expect(jobs.filter((j: any) => j.status === "completed")).toHaveLength(3);
  expect(ref.id).toBeTruthy();
  await page.locator(".asset-open").first().click();
  await page
    .getByRole("button", { name: "Als Variante verwenden", exact: true })
    .click();
  await page.getByText("Ansicht, Pose und Mimik", { exact: true }).click();
  await expect(page.getByLabel("Pose", { exact: true })).toHaveValue(
    assets.items[0].parameters.blocks.pose,
  );
  await expect(page.locator(".reference-chip")).toHaveCount(2);
});
test("1,000 actual imported assets: bounded gallery DOM, paging, render timing and browser heap", async ({
  page,
  context,
}) => {
  let report: any;
  try {
    report = JSON.parse(
      await fs.readFile(".artifacts/scale-report.json", "utf8"),
    );
  } catch {
    test.skip(
      true,
      "Run scripts/acceptance.py first to produce the 1,000-asset fixture.",
    );
  }
  const store = report.data_dir;
  const p = (await call("project.list", {}, store)).find(
    (p: any) => p.id === report.scale_project,
  );
  expect(p).toBeTruthy();
  await routeCore(page, store);
  await call("settings.save", { theme: "light" }, store);
  const start = performance.now();
  await page.goto("/");
  await page.getByRole("button", { name: p.name, exact: true }).click();
  await expect(page.locator(".asset-card")).toHaveCount(72);
  await expect(page.locator(".gallery-heading .count")).toHaveText("1000");
  await expect(page.locator(".asset-card img").first()).toBeVisible();
  const galleryReady = performance.now() - start;
  const frames = await page.evaluate(
    () =>
      new Promise<number[]>((resolve) => {
        const out: number[] = [];
        let previous = performance.now();
        function frame(now: number) {
          out.push(now - previous);
          previous = now;
          if (out.length < 60) requestAnimationFrame(frame);
          else resolve(out.slice(1));
        }
        requestAnimationFrame(frame);
      }),
  );
  await page.screenshot({
    path: path.join(proof, "10-gallery-1000-light.png"),
  });
  const navStart = performance.now();
  await page.getByRole("button", { name: "Weiter", exact: true }).click();
  await expect(page.locator(".pagination>span")).toHaveText("73–144 / 1000");
  await expect(page.locator(".asset-card")).toHaveCount(72);
  const navigationMs = performance.now() - navStart;
  const cdp = await context.newCDPSession(page);
  await cdp.send("Performance.enable");
  const metrics = await cdp.send("Performance.getMetrics");
  const heap = metrics.metrics.find(
    (m: any) => m.name === "JSHeapUsedSize",
  )?.value;
  const documentCount = await page.locator("*").count();
  const screenshotName = path.join(proof, "11-gallery-1000-page2.png");
  await page.screenshot({ path: screenshotName });
  await fs.writeFile(
    path.join(proof, "scale-metrics.json"),
    JSON.stringify(
      {
        dataset: store,
        total_assets: 1000,
        rendered_cards: 72,
        gallery_ready_ms: Number(galleryReady.toFixed(2)),
        next_page_ms: Number(navigationMs.toFixed(2)),
        animation_frame_mean_ms: Number(
          (frames.reduce((a, b) => a + b, 0) / frames.length).toFixed(2),
        ),
        animation_frame_max_ms: Number(Math.max(...frames).toFixed(2)),
        chromium_js_heap_bytes: heap,
        dom_elements: documentCount,
        transport:
          "Actual Rust core via trusted CLI per RPC; includes subprocess overhead, not native Tauri benchmark",
        viewport: { width: 1440, height: 1000 },
      },
      null,
      2,
    ),
  );
});


test("OpenAI: separate key controls, capabilities and quality survive provider/model switching without paid jobs", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/openai-ui-"));
  await call("settings.save", { theme: "light", mode: "expert" }, store);
  await call("project.save", { name: "Synthetischer Mehranbieter-Test" }, store);
  const state = await call("state", {}, store);
  const openaiModels = state.models.filter((m: any) => m.provider === "openai");
  expect(openaiModels.length).toBeGreaterThan(0);
  await routeCore(page, store, false, ["gemini", "openai"]);
  await page.goto("/");
  await page.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true }).fill("Synthetischer Test: keine Generierung starten.");
  await page.getByLabel("Provider", { exact: true }).selectOption("gemini");
  await page.getByLabel("Seitenverhältnis", { exact: true }).selectOption("16:9");
  await page.getByLabel("Native Auflösung", { exact: true }).selectOption("4K");
  const openaiEstimate = page.waitForRequest((r) => r.url().endsWith("/rpc") && r.postDataJSON()?.op === "cost.estimate" && r.postDataJSON()?.args?.provider === "openai");
  await page.getByLabel("Provider", { exact: true }).selectOption("openai");
  expect((await openaiEstimate).postDataJSON().args.quality).toBe("medium");
  await expect(page.getByLabel("Modell", { exact: true })).toHaveValue(openaiModels[0].id);
  await expect(page.getByLabel("Native Auflösung", { exact: true })).toHaveValue("1K");
  await expect(page.getByLabel("Seitenverhältnis", { exact: true })).toHaveValue("1:1");
  await expect(page.getByText(/nicht unterstützte Einstellungen angepasst/)).toBeVisible();
  await expect(page.getByLabel("Bildqualität", { exact: true })).toHaveValue("medium");
  await page.getByLabel("Bildqualität", { exact: true }).selectOption("high");
  await page.getByLabel("Seitenverhältnis", { exact: true }).selectOption("3:2");
  if (openaiModels.length > 1) {
    await page.getByLabel("Modell", { exact: true }).selectOption(openaiModels[1].id);
    await expect(page.getByLabel("Bildqualität", { exact: true })).toHaveValue("high");
    await expect(page.getByLabel("Seitenverhältnis", { exact: true })).toHaveValue("3:2");
  }
  await page.getByRole("button", { name: "Einfach", exact: true }).click();
  await expect(page.getByText(/Bildqualität: Hoch/)).toBeVisible();
  await page.getByRole("button", { name: "Experte", exact: true }).click();
  await expect(page.getByLabel("Bildqualität", { exact: true })).toHaveValue("high");
  await expect(page.locator(".estimate strong")).not.toHaveText("—");
  await page.screenshot({ path: path.join(proof, "12-openai-model-controls.png") });
  const testEstimate = page.waitForRequest((r) => r.url().endsWith("/rpc") && r.postDataJSON()?.op === "cost.estimate" && r.postDataJSON()?.args?.provider === "gemini");
  await page.getByLabel("Provider", { exact: true }).selectOption("gemini");
  expect((await testEstimate).postDataJSON().args).not.toHaveProperty("quality");
  await expect(page.getByLabel("Bildqualität", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Seitenverhältnis", { exact: true })).toHaveValue("3:2");
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"API-Schlüssel",exact:true}).click();
  const selector = page.getByLabel("Anbieter für Schlüsselablage", { exact: true });
  await expect(page.getByLabel("Gemini API-Schlüssel", { exact: true })).toHaveValue("");
  await page.getByLabel("Gemini API-Schlüssel", { exact: true }).fill("not-a-real-key-not-submitted");
  await selector.selectOption("openai");
  await expect(page.getByLabel("OpenAI API-Schlüssel", { exact: true })).toHaveValue("");
  await expect(page.getByText(/eine ChatGPT-Anmeldung allein reicht nicht/)).toBeVisible();
  await page.getByLabel("OpenAI API-Schlüssel", { exact: true }).fill("short");
  const rejectedKey = page.waitForRequest((r) => r.url().endsWith("/rpc") && r.postDataJSON()?.op === "secret.set");
  await page.getByRole("button", { name: "Schlüssel sicher speichern", exact: true }).click();
  expect((await rejectedKey).postDataJSON().args.provider).toBe("openai");
  await expect(page.getByRole("alert")).toContainText("Ungültiges Schlüsselformat");
  await expect(page.getByLabel("OpenAI API-Schlüssel", { exact: true })).toHaveValue("");
  await selector.selectOption("gemini");
  await expect(page.getByLabel("Gemini API-Schlüssel", { exact: true })).toHaveValue("");
  await selector.selectOption("openai");
  await page.screenshot({ path: path.join(proof, "13-provider-key-settings.png") });
  expect(await call("job.list", {}, store)).toHaveLength(0);
  expect((await call("state", {}, store)).key_status).toEqual(state.key_status);
});

test("Style previews: prepare without spend, generate, verify before adopting, persist, version and remove without deleting assets", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/style-preview-ui-"));
  await call("settings.save", { theme: "light", mode: "simple" }, store);
  const previewWorker = spawn(binary, ["--data-dir", store, "--worker"], { stdio: ["ignore", "pipe", "pipe"] });
  previewWorker.stderr.on("data", () => {});
  try {
    await routeCore(page, store);
    await page.goto("/");
    await page.getByRole("button", { name: /^Stilbibliothek/ }).click();
    const card = page.locator(".library-card").filter({ has: page.getByRole("heading", { name: "Aquarell", exact: true }) });
    await expect(card.getByRole("img", { name: "Mitgelieferte Stilvorschau: Aquarell", exact: true })).toBeVisible();
    await card.getByRole("button", { name: "Vorschau gestalten", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByRole("heading", { name: "Stilvorschau · Aquarell", exact: true })).toBeVisible();
    await expect(dialog.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })).not.toHaveValue("");
    await expect(dialog.getByLabel("Stil", { exact: true })).toBeDisabled();
    await expect(dialog.getByLabel("Provider", { exact: true })).toHaveValue("test");
    await expect(dialog.locator(".prompt-preview pre")).toContainText("Wasserfarbe");
    await expect(dialog.locator(".estimate strong")).not.toHaveText("—");
    expect(await call("job.list", {}, store)).toHaveLength(0);
    const projects = await call("project.list", {}, store);
    expect(projects).toHaveLength(1);
    expect(projects[0].name).toBe("Stilvorschauen");
    await page.screenshot({ path: path.join(proof, "14-style-preview-prepared.png") });
    await dialog.getByRole("button", { name: "1 Bild erzeugen", exact: true }).click();
    const adopt = dialog.getByRole("button", { name: "Als Stilvorschau übernehmen", exact: true });
    await expect(adopt).toBeVisible();
    await expect(dialog.getByText("Testbild · keine Stilbewertung", { exact: true })).toBeVisible();
    const gallery = await call("asset.list", { project_id: projects[0].id }, store);
    expect(gallery.items).toHaveLength(1);
    const asset = await call("asset.get", { id: gallery.items[0].id }, store);
    expect((await call("style.list", {}, store)).find((s: any) => s.id === asset.snapshot.style.id).preview_asset_id).toBeFalsy();
    // Real core failure path with a temporarily unavailable synthetic original.
    const original = path.join(store, asset.path);
    const hidden = original + ".temporarily-unavailable";
    await fs.rename(original, hidden);
    try {
      await adopt.click();
      await expect(dialog.getByRole("alert")).toBeVisible();
      expect((await call("style.list", {}, store)).find((s: any) => s.id === asset.snapshot.style.id).preview_asset_id).toBeFalsy();
    } finally {
      await fs.rename(hidden, original);
    }
    await page.screenshot({ path: path.join(proof, "15-style-preview-reviewed.png") });
    await adopt.click();
    await expect(dialog).toHaveCount(0);
    await expect(card.getByRole("img", { name: "Stilvorschau: Aquarell", exact: true })).toBeVisible();
    await expect(card.getByText("Testbild · keine Stilbewertung", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Stilvorschauen", exact: true }).click();
    await page.getByLabel("Stil", { exact: true }).selectOption(asset.snapshot.style.id);
    await expect(page.locator(".generator-style-preview").getByRole("img", { name: "Stilvorschau: Aquarell", exact: true })).toBeVisible();
    await expect(page.locator(".generator-style-preview").getByText("Testbild · keine Stilbewertung", { exact: true })).toBeVisible();
    await page.reload();
    await page.getByRole("button", { name: /^Stilbibliothek/ }).click();
    await expect(card.getByRole("img", { name: "Stilvorschau: Aquarell", exact: true })).toBeVisible();
    await card.getByRole("button", { name: "Bearbeiten", exact: true }).click();
    const editor = page.getByRole("dialog");
    const motif = editor.getByLabel("Vorschaumotiv", { exact: true });
    await motif.fill((await motif.inputValue()) + " Die Zimmerpflanze steht links auf der Werkbank.");
    await editor.getByRole("button", { name: "Speichern", exact: true }).click();
    await expect(card.getByText("Vorschau einer älteren Version", { exact: true })).toBeVisible();
    await page.screenshot({ path: path.join(proof, "16-style-preview-library.png") });
    await card.getByRole("button", { name: "Vorschau entfernen", exact: true }).click();
    await expect(card.getByText("Noch keine Stilvorschau", { exact: true })).toBeVisible();
    expect((await call("asset.get", { id: asset.id }, store)).id).toBe(asset.id);
    await expect.poll(async () => (await call("job.list", {}, store)).length).toBe(1);
  } finally {
    previewWorker.kill("SIGTERM");
  }
});

test("Pricing: three real catalogs, Anthropic key isolation and failed manual refresh retain valid prices", async ({ page, context }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/pricing-ui-"));
  await call("settings.save", { theme: "light" }, store);
  const original = await call("pricing.catalog", {}, store);
  const state = await call("state", {}, store);
  // The real Rust HTTP client is pointed at a closed local proxy for an offline failure case.
  // No price data, RPC response or frontend state is fabricated.
  await routeCore(page, store, true);
  await page.goto("/");
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"Preise & Anbieter",exact:true}).click();
  await expect(page.locator(".pricing-provider")).toHaveCount(3);
  const anthropic = page.locator(".pricing-provider").filter({ has: page.getByRole("heading", { name: "Anthropic / Claude", exact: true }) });
  await expect(anthropic).toContainText("Keine Bildgenerierung");
  await expect(anthropic.locator("table")).toHaveCount(0);
  await expect(anthropic.locator(".pricing-rates")).toContainText("Texteingabe");
  for (const provider of original.providers) {
    const card = page.locator(".pricing-provider").filter({ has: page.locator(".pricing-source-url", { hasText: provider.source }) });
    await expect(card).toHaveCount(1);
    for (const model of provider.models) await expect(card).toContainText(model.price_snapshot.version);
  }
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await anthropic.getByRole("button", { name: "Quellenadresse kopieren", exact: true }).click();
  await expect(anthropic.getByRole("button", { name: "Kopiert", exact: true })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(original.providers.find((p: any) => p.id === "anthropic").source);
  await page.screenshot({ path: path.join(proof, "17-provider-pricing-light.png") });
  const refreshRequest = page.waitForRequest((r) => r.postDataJSON()?.op === "pricing.refresh");
  await page.getByRole("button", { name: "Preise aktualisieren", exact: true }).click();
  expect((await refreshRequest).postDataJSON().args).toEqual({});
  await expect(page.getByRole("status")).toContainText("Nicht alle Anbieter konnten aktualisiert werden");
  await expect(page.locator(".pricing-provider-heading").getByText("Letzter gültiger Preisstand", { exact: true })).toHaveCount(3);
  const retained = await call("pricing.catalog", {}, store);
  for (const provider of retained.providers) {
    const old = original.providers.find((p: any) => p.id === provider.id);
    expect(provider.status).toBe("error");
    expect(provider.checked_at).toBe(old.checked_at);
    expect(provider.models).toEqual(old.models);
    expect(provider.last_attempt_at).toBeTruthy();
  }
  await anthropic.scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.join(proof, "18-provider-pricing-retained.png") });
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"Preise & Anbieter",exact:true}).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"API-Schlüssel",exact:true}).click();
  const selector = page.getByLabel("Anbieter für Schlüsselablage", { exact: true });
  await page.getByLabel("Gemini API-Schlüssel", { exact: true }).fill("not-a-real-key-not-submitted");
  await selector.selectOption("anthropic");
  const key = page.getByLabel("Anthropic API-Schlüssel", { exact: true });
  await expect(key).toHaveValue("");
  await key.fill("short");
  const secretRequest = page.waitForRequest((r) => r.postDataJSON()?.op === "secret.set");
  await page.getByRole("button", { name: "Schlüssel sicher speichern", exact: true }).click();
  expect((await secretRequest).postDataJSON().args.provider).toBe("anthropic");
  await expect(page.getByRole("alert")).toContainText("Ungültiges Schlüsselformat");
  await expect(key).toHaveValue("");
  await selector.selectOption("openai");
  await expect(page.getByLabel("OpenAI API-Schlüssel", { exact: true })).toHaveValue("");
  expect((await call("state", {}, store)).key_status).toEqual(state.key_status);
  expect(await call("job.list", {}, store)).toHaveLength(0);
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"Darstellung",exact:true}).click();
  await page.getByLabel("Erscheinungsbild", { exact: true }).selectOption("dark");
  await page.getByRole("dialog").getByRole("button", { name: "Speichern", exact: true }).click();
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"Preise & Anbieter",exact:true}).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.locator(".pricing-provider").first().scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.join(proof, "21-provider-pricing-dark.png") });
});

test("Prompt assistant: explicit local request, exact input, reviewed result and durable retrieval without image generation", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/assistant-ui-"));
  await call("settings.save", { theme: "light", mode: "expert" }, store);
  const project = await call("project.save", { name: "Synthetischer Textassistent" }, store);
  const state = await call("state", {}, store);
  const style = state.styles.find((s: any) => s.name === "Aquarell");
  await routeCore(page, store);
  await page.goto("/");
  const description = "Ein Roboter hält eine weiße Kaffeetasse. <script>window.assistantUnsafe=true</script>";
  await page.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true }).fill(description);
  await page.getByLabel("Stil", { exact: true }).selectOption(style.id);
  await expect(page.locator(".prompt-preview pre")).toContainText("Wasserfarbe");
  const beforePrompt = await page.locator(".prompt-preview pre").innerText();
  await page.getByRole("button", { name: "Prompt mit Claude verbessern", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByLabel("Beschreibung, die gesendet wird", { exact: true })).toHaveValue(description);
  await expect(dialog.getByLabel("Prompt-Assistent wählen", { exact: true })).toHaveValue("anthropic");
  await expect(dialog.locator(".assistant-footer strong")).not.toHaveText("—");
  expect(await call("job.list", {}, store)).toHaveLength(0);
  await dialog.getByLabel("Änderungswunsch (optional)", { exact: true }).fill("Die Komposition klarer beschreiben.");
  await dialog.getByLabel("Prompt-Assistent wählen", { exact: true }).selectOption("test");
  const submit = dialog.getByRole("button", { name: "Vorschlag anfordern", exact: true });
  await expect(submit).toBeEnabled();
  const request = page.waitForRequest((r) => r.postDataJSON()?.op === "assistant.submit");
  await submit.click();
  const submitted = (await request).postDataJSON().args;
  expect(submitted.text).toBe(description);
  expect(submitted.instruction).toBe("Die Komposition klarer beschreiben.");
  expect(submitted.provider).toBe("test");
  expect(submitted.expected_price_version).toBe("test-assistant-v1");
  expect(submitted).not.toHaveProperty("style_id");
  expect(submitted).not.toHaveProperty("reference_ids");
  await expect.poll(async () => (await call("job.list", {}, store)).length).toBe(1);
  const queued = await call("job.list", {}, store);
  expect(queued).toHaveLength(1);
  expect(queued[0].kind).toBe("prompt_assist");
  expect(queued[0].status).toBe("queued");
  await expect(dialog.getByRole("button", { name: "Auftrag eingereiht", exact: true })).toBeDisabled();
  await page.screenshot({ path: path.join(proof, "19-assistant-queued-review.png") });
  const textWorker = spawn(binary, ["--data-dir", store, "--worker"], { stdio: ["ignore", "pipe", "pipe"] });
  textWorker.stderr.on("data", () => {});
  try {
    const apply = dialog.getByRole("button", { name: "Beschreibung übernehmen", exact: true });
    await expect(apply).toBeVisible();
    const result = await dialog.getByLabel("Ergebnis des Prompt-Assistenten", { exact: true }).inputValue();
    expect(result).toContain(description);
    await expect(page.locator(".generator").getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })).toHaveValue(description);
    expect(await page.evaluate(() => (window as any).assistantUnsafe)).toBeUndefined();
    await page.screenshot({ path: path.join(proof, "20-assistant-result-review.png") });
    await apply.click();
    await expect(dialog).toHaveCount(0);
    await expect(page.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })).toHaveValue(result);
    await expect(page.getByLabel("Stil", { exact: true })).toHaveValue(style.id);
    expect(beforePrompt).toContain("Wasserfarbe");
    await expect(page.locator(".prompt-preview pre")).toContainText("Wasserfarbe");
    expect((await call("asset.list", { project_id: project.id }, store)).items).toHaveLength(0);
    expect(await call("job.list", {}, store)).toHaveLength(1);
    await page.reload();
    await page.getByRole("button", { name: /^Aufträge/ }).click();
    await page.getByRole("button", { name: "Vorschlag ansehen", exact: true }).click();
    await expect(page.getByRole("dialog").getByLabel("Ergebnis des Prompt-Assistenten", { exact: true })).toHaveValue(result);
    await page.getByRole("dialog").getByRole("button", { name: "Beschreibung übernehmen", exact: true }).click();
    await expect(page.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true })).toHaveValue(result);
    await call("settings.save", { theme: "dark" }, store);
    await page.reload();
    await expect(page.getByRole("button", { name: "Prompt mit Claude verbessern", exact: true })).toBeVisible();
    await page.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true }).fill(result);
    await page.getByRole("button", { name: "Prompt mit Claude verbessern", exact: true }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(page.getByRole("dialog").locator(".assistant-footer strong")).not.toHaveText("—");
    await page.screenshot({ path: path.join(proof, "22-assistant-dark.png") });
    expect(await call("job.list", {}, store)).toHaveLength(1);
    const costs = await call("cost.project", { project_id: project.id }, store);
    expect(costs.by_kind.prompt_assist.calculated_microusd).toBe(0);
  } finally { textWorker.kill("SIGTERM"); }
});


for (const fixture of [
  { provider: "Gemini", code: "HTTP 400 · INVALID_ARGUMENT", error: "Google Gemini: HTTP 400 · INVALID_ARGUMENT. Das angeforderte direkte Ausgabeformat wurde abgelehnt.", screenshot: "27-style-error-unknown-cost.png" },
  { provider: "OpenAI", code: "HTTP 429 · credit_balance_exhausted", error: "OpenAI: HTTP 429 · credit_balance_exhausted. Das API-Guthaben der OpenAI-Organisation ist aufgebraucht. Guthaben in den API-Abrechnungseinstellungen prüfen.", screenshot: "28-openai-error-unknown-cost.png" },
]) test(`Style failure UI ${fixture.provider}: synthetic HTTP rejection shows unknown actual cost and separate estimate without retry`, async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/style-error-ui-"));
  const styles = await call("style.list", {}, store);
  const style = styles.find((entry: any) => entry.name === "Comic mit klarer Kontur");
  const prepared = await call("style.preview.prepare", { id: style.id }, store);
  await routeCore(page, store);
  let submissions = 0;
  await page.route("**/rpc", async (route) => {
    const request = route.request().postDataJSON();
    if (request.op === "job.submit") submissions += 1;
    if (request.op !== "job.list") { await route.fallback(); return; }
    // Explicit UI-only error fixture. No provider request or persisted failed job.
    const job = { id: "synthetic-http-rejection", project_id: prepared.project.id, status: "failed", cost_status: "unknown", actual_microusd: null, estimated_microusd: 50800, snapshot: { style: { id: style.id } }, error: fixture.error };
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify([job]) });
  });
  await page.goto("/");
  await page.getByRole("button", { name: /^Stilbibliothek/ }).click();
  const card = page.locator(".library-card").filter({ has: page.getByRole("heading", { name: "Comic mit klarer Kontur", exact: true }) });
  await card.getByRole("button", { name: "Vorschau gestalten", exact: true }).click();
  const row = page.locator(".style-preview-job");
  await expect(row).not.toBeVisible();
  await page.getByText("Frühere Aufträge anzeigen (1)", { exact: true }).click();
  await expect(row).toBeVisible();
  await expect(row).toContainText(fixture.code);
  await expect(row.locator("small")).toContainText("Kosten unbekannt");
  await expect(row.locator("small")).toContainText("Geschätzt:");
  await row.scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.join(proof, fixture.screenshot) });
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  await card.getByRole("button", { name: "Vorschau gestalten", exact: true }).click();
  await expect(row).not.toBeVisible();
  expect(submissions).toBe(0);
  expect(await call("job.list", {}, store)).toHaveLength(0);
});

test("Series approval: one reviewed approval runs three frozen images, persists results, and can reject a later request", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/series-ui-"));
  await call("settings.save", { theme: "light" }, store);
  const project = await call("project.save", { name: "Drei Werkstattszenen" }, store);
  const args = { project_id: project.id, description: "Drei Kapitelbilder für die Werkstatt-Präsentation", idempotency_key: "series-ui", items: [
    { description: "Kapitel 1 · Ankunft in der Werkstatt", provider: "test" },
    { description: "Kapitel 2 · Der Roboter baut", provider: "test" },
    { description: "Kapitel 3 · Gemeinsame Präsentation", provider: "test" },
  ] };
  const series = await call("series.submit", args, store);
  await routeCore(page, store);
  let approvals = 0;
  await page.route("**/rpc", async route => { if (route.request().postDataJSON().op === "series.approve") approvals++; await route.fallback(); });
  await page.goto("/");
  await page.getByRole("button", { name: "Serienfreigaben: 1", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText(args.description);
  await expect(dialog).toContainText("3 Bilder");
  await expect(dialog).toContainText("Herkunft");
  await expect(dialog).toContainText("Lokal");
  await dialog.getByText("Geplante Bilder und Prompts (3)", { exact: true }).click();
  for (const item of args.items) await expect(dialog).toContainText(item.description);
  expect(await call("job.list", {}, store)).toHaveLength(0);
  await page.screenshot({ path: path.join(proof, "29-series-approval-light.png") });
  await page.evaluate(() => { document.documentElement.dataset.theme = "dark"; });
  await page.screenshot({ path: path.join(proof, "30-series-approval-dark.png") });
  await page.evaluate(() => { document.documentElement.dataset.theme = "light"; });
  await dialog.getByRole("button", { name: "Gesamte Serie freigeben · 3 Bilder", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator(".canvas-heading h2")).toHaveText(project.name);
  const drawer = page.getByRole("region", { name: "Aufträge", exact: true });
  await expect(drawer).toBeVisible();
  await expect(drawer).toContainText(args.description);
  await expect(drawer.locator(".job-row")).toHaveCount(3);
  expect(approvals).toBe(1);
  const queue = await call("job.list", {}, store);
  expect(queue).toHaveLength(3);
  expect(queue.every((job: any) => job.series_id === series.id && job.status === "queued")).toBe(true);
  const seriesWorker = spawn(binary, ["--data-dir", store, "--worker"], { stdio: ["ignore", "pipe", "pipe"] });
  try {
    await expect.poll(async () => (await call("series.get", { id: series.id }, store)).job_status_counts.completed).toBe(3);
    await expect(drawer).toContainText("3 × Fertig");
    expect((await call("asset.list", { project_id: project.id }, store)).total).toBe(3);
    await page.reload();
    await page.getByRole("button", { name: "Serienfreigaben: 0", exact: true }).click();
    await expect(page.getByRole("dialog")).toContainText("3 × Fertig");
    await call("series.submit", { ...args, idempotency_key: "reject-ui", description: "Weitere Serie zur Ablehnung" }, store);
    const card = page.locator(".series-approval-card").filter({ has: page.getByRole("heading", { name: "Weitere Serie zur Ablehnung" }) });
    await card.getByRole("button", { name: "Ablehnen", exact: true }).click();
    await expect(card).toContainText("Abgelehnt");
    expect(await call("job.list", {}, store)).toHaveLength(3);
  } finally { seriesWorker.kill("SIGTERM"); }
});

for (const provider of ["gemini", "openai"]) test(`Configured ${provider}: hide test provider, disable missing key, and block submission when local read fails`, async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/provider-ready-ui-"));
  await routeCore(page, store, false, [provider]);
  let checks = 0;
  let submissions = 0;
  await page.route("**/rpc", async route => {
    const req = route.request().postDataJSON();
    if (req.op === "secret.check") {
      checks++;
      expect(req.args.provider).toBe(provider);
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: "Synthetischer Zugriffsfehler: App-Freigabe fehlt. Nur für diese Sitzung verwenden." }) });
      return;
    }
    if (req.op === "job.submit") {
      submissions++;
      throw new Error("Paid submission forbidden in this fixture");
    }
    await route.fallback();
  });
  await page.goto("/");
  await page.getByRole("button", { name: /^Stilbibliothek/ }).click();
  const card = page.locator(".library-card").filter({ has: page.getByRole("heading", { name: "Aquarell", exact: true }) });
  await card.getByRole("button", { name: "Vorschau gestalten", exact: true }).click();
  const selector = page.getByLabel("Provider", { exact: true });
  await expect(selector).toHaveValue(provider);
  await expect(selector.locator('option[value="test"]')).toHaveCount(0);
  await expect(selector.locator(`option[value="${provider === "gemini" ? "openai" : "gemini"}"]`)).toBeDisabled();
  await page.getByRole("button", { name: "1 Bild erzeugen", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("App-Freigabe fehlt");
  expect(checks).toBe(1);
  expect(submissions).toBe(0);
  expect(await call("job.list", {}, store)).toHaveLength(0);
});

test("Style dialog keeps current failures visible but collapses them after reopening", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/current-failure-ui-"));
  await routeCore(page, store);
  let job: any;
  await page.route("**/rpc", async route => {
    const req = route.request().postDataJSON();
    if (req.op === "job.submit") {
      job = { id: "synthetic-current-failure", status: "failed", cost_status: "not_sent", actual_microusd: 0, snapshot: { style: { id: req.args.style_id } }, error: "Synthetischer aktueller Fehler" };
      await route.fulfill({ json: { jobs: [job] } }); return;
    }
    if (req.op === "job.list") { await route.fulfill({ json: job ? [job] : [] }); return; }
    await route.fallback();
  });
  await page.goto("/");
  await page.getByRole("button", { name: /^Stilbibliothek/ }).click();
  const card = page.locator(".library-card").filter({ has: page.getByRole("heading", { name: "Aquarell", exact: true }) });
  await card.getByRole("button", { name: "Vorschau gestalten", exact: true }).click();
  await page.getByRole("button", { name: "1 Bild erzeugen", exact: true }).click();
  await expect(page.getByText("Synthetischer aktueller Fehler", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  await card.getByRole("button", { name: "Vorschau gestalten", exact: true }).click();
  await expect(page.getByText("Frühere Aufträge anzeigen (1)", { exact: true })).toBeVisible();
  await expect(page.getByText("Synthetischer aktueller Fehler", { exact: true })).not.toBeVisible();
});


test("Configured provider remains selected after restoring or changing the current project", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/provider-project-ui-"));
  await call("project.save", { name: "Projekt Eins" }, store);
  await call("project.save", { name: "Projekt Zwei" }, store);
  await routeCore(page, store, false, ["gemini"]);
  const providers: string[] = [];
  await page.route("**/rpc", async route => {
    const req = route.request().postDataJSON();
    if (req.op === "cost.estimate") providers.push(req.args.provider);
    await route.fallback();
  });
  await page.goto("/");
  for (const name of ["Projekt Eins", "Projekt Zwei"]) {
    await page.getByRole("button", { name, exact: true }).click();
    await expect(page.getByText(/Testprovider: lokale, deterministische Prüfbilder/)).toHaveCount(0);
    await page.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true }).fill(`Synthetischer Entwurf ${name}, niemals senden`);
    await expect(page.locator(".estimate strong")).not.toHaveText("—");
    expect(providers.at(-1)).toBe("gemini");
    await expect(page.getByRole("button", { name: "1 Bild erzeugen", exact: true })).toBeEnabled();
  }
  expect(await call("job.list", {}, store)).toHaveLength(0);
});


test("Project navigation: overview, create two, keyboard open, rename, duplicate, archive, restore, search and persistence", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/project-navigation-ui-"));
  await call("settings.save", { theme: "light" }, store);
  await routeCore(page, store);
  await page.goto("/");
  const projectsNav = page.getByRole("navigation", { name: "Bibliotheken" }).getByRole("button", { name: /^Projekte/ });
  const overview = page.getByRole("region", { name: "Projektübersicht", exact: true });
  await projectsNav.click();
  await expect(overview.getByRole("heading", { name: "Noch keine aktiven Projekte", exact: true })).toBeVisible();
  async function create(name: string, description: string) {
    await overview.getByRole("button", { name: "Neues Projekt", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Projektname", { exact: true }).fill(name);
    await dialog.getByLabel(/^(Beschreibung|Einzelanweisung)$/, { exact: true }).fill(description);
    await dialog.getByRole("button", { name: "Speichern", exact: true }).click();
    await expect(page.locator(".canvas-heading h2")).toHaveText(name);
    await expect(overview).toHaveCount(0);
  }
  await create("Test", "Präsentationsgrafiken für das erste Projekt");
  await projectsNav.click();
  await create("Zweites Projekt", "Charaktere und Szenen für das zweite Projekt");
  await page.getByRole("button", { name: /^Stilbibliothek/ }).click();
  await projectsNav.click();
  await expect(overview.getByRole("button", { name: /^Projekt öffnen:/ })).toHaveCount(2);
  await expect(overview.getByRole("heading", { name: "Projekte 2", exact: true })).toBeVisible();
  await expect(page.locator(".workbench")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Referenz importieren", exact: true })).toHaveCount(0);
  await page.screenshot({ path: path.join(proof, "31-project-overview-light.png") });
  await overview.getByLabel("Projekte suchen", { exact: true }).fill("Charaktere");
  await expect(overview.getByRole("button", { name: /^Projekt öffnen:/ })).toHaveCount(1);
  await overview.getByLabel("Projekte suchen", { exact: true }).fill("unauffindbar");
  await expect(overview.getByText("Keine passenden Projekte", { exact: true })).toBeVisible();
  await overview.getByLabel("Projekte suchen", { exact: true }).fill("");
  await overview.getByRole("button", { name: "Projekt öffnen: Test", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".canvas-heading h2")).toHaveText("Test");
  await page.getByRole("button", { name: "Projektaktionen", exact: true }).click();
  await page.getByRole("button", { name: "Projektvorgaben", exact: true }).click();
  await page.getByLabel("Projektname", { exact: true }).fill("Test umbenannt");
  await page.getByRole("dialog").getByRole("button", { name: "Speichern", exact: true }).click();
  await expect(page.locator(".canvas-heading h2")).toHaveText("Test umbenannt");
  await page.getByRole("button", { name: "Projektaktionen", exact: true }).click();
  await page.getByRole("button", { name: "Duplizieren", exact: true }).click();
  await expect(page.locator(".canvas-heading h2")).toHaveText("Test umbenannt · Kopie");
  const duplicate = (await call("project.list", {}, store)).find((project: any) => !["Test umbenannt", "Zweites Projekt"].includes(project.name));
  await expect(page.locator(".canvas-heading h2")).toHaveText(duplicate.name);
  await page.getByRole("button", { name: "Projektaktionen", exact: true }).click();
  await page.getByRole("button", { name: "Archivieren", exact: true }).click();
  await projectsNav.click();
  await expect(overview.getByRole("button", { name: /^Projekt öffnen:/ })).toHaveCount(2);
  await overview.getByRole("button", { name: "Archiv anzeigen", exact: true }).click();
  await expect(overview.getByRole("button", { name: /^Projekt öffnen:/ })).toHaveCount(1);
  await overview.getByRole("button", { name: `Projekt öffnen: ${duplicate.name}`, exact: true }).click();
  await page.getByRole("button", { name: "Projektaktionen", exact: true }).click();
  await page.getByRole("button", { name: "Wiederherstellen", exact: true }).click();
  await projectsNav.click();
  await expect(overview.getByRole("button", { name: /^Projekt öffnen:/ })).toHaveCount(3);
  await overview.getByRole("button", { name: "Archiv anzeigen", exact: true }).click();
  await expect(overview.getByRole("heading", { name: "Das Archiv ist leer.", exact: true })).toBeVisible();
  await overview.getByRole("button", { name: "Aktive Projekte", exact: true }).click();
  await call("settings.save", { theme: "dark" }, store);
  await page.reload();
  await projectsNav.click();
  await expect(overview.getByRole("button", { name: /^Projekt öffnen:/ })).toHaveCount(3);
  await page.screenshot({ path: path.join(proof, "32-project-overview-dark.png") });
  await page.setViewportSize({ width: 1000, height: 760 });
  await expect(overview.getByRole("button", { name: "Projekt öffnen: Zweites Projekt", exact: true })).toBeVisible();
  await page.screenshot({ path: path.join(proof, "33-project-overview-compact.png") });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(await call("job.list", {}, store)).toHaveLength(0);
});

test("Reference photo upload: original bytes, automatic selection, limits, invalid file and persistence", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/reference-upload-ui-"));
  const project = await call("project.save", { name: "Foto-Referenzen" }, store);
  await routeCore(page, store);
  await page.goto("/");
  await page.getByRole("button", { name: "Foto-Referenzen", exact: true }).click();
  await page.locator(".add-reference").click();
  const dialog = page.getByRole("dialog", { name: "Referenzbilder auswählen" });
  const bytes = Buffer.from(await page.evaluate(() => {
    const canvas = document.createElement("canvas"); canvas.width = 80; canvas.height = 120;
    const context = canvas.getContext("2d")!; context.fillStyle = "#b44b2b"; context.fillRect(0, 0, 80, 120);
    return canvas.toDataURL("image/png").split(",")[1];
  }), "base64");
  const input = dialog.getByLabel("Referenzfotos hochladen", { exact: true });
  await input.setInputFiles({ name: "synthetisches-portrait.png", mimeType: "image/png", buffer: bytes });
  await expect(dialog.getByRole("button", { name: "synthetisches-portrait", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(dialog.getByRole("status")).toContainText("1 Referenzen");
  const assets = (await call("asset.list", { project_id: project.id }, store)).items;
  expect(assets).toHaveLength(1);
  expect(assets[0].status).toBe("reference");
  const original = await call("asset.read", { id: assets[0].id }, store);
  expect(Buffer.from(original.data_base64, "base64")).toEqual(bytes);
  expect(await call("job.list", {}, store)).toHaveLength(0);
  await page.screenshot({ path: path.join(proof, "34-reference-upload.png") });
  await input.setInputFiles(Array.from({length: 8}, (_, i) => ({ name: `limit-${i}.png`, mimeType: "image/png", buffer: bytes })));
  await expect(dialog.getByRole("alert")).toContainText("Maximale Anzahl");
  expect((await call("asset.list", { project_id: project.id }, store)).items).toHaveLength(1);
  await input.setInputFiles({ name: "ungueltig.png", mimeType: "image/png", buffer: Buffer.from("not an image") });
  await expect(dialog.getByRole("alert")).toBeVisible();
  await expect(dialog.getByRole("alert")).not.toContainText("Maximale Anzahl");
  await expect(dialog.getByRole("button", { name: "synthetisches-portrait", exact: true })).toHaveAttribute("aria-pressed", "true");
  await dialog.getByRole("button", { name: "Schließen", exact: true }).last().click();
  await expect(page.locator(".reference-chip")).toHaveCount(1);
  await page.reload();
  await expect(page.locator(".reference-chip")).toHaveCount(0);
  await page.locator(".add-reference").click();
  await expect(dialog.getByRole("button", { name: "synthetisches-portrait", exact: true })).toHaveAttribute("aria-pressed", "false");
  await dialog.getByRole("button", { name: "synthetisches-portrait", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "synthetisches-portrait", exact: true })).toHaveAttribute("aria-pressed", "true");
  expect(await call("job.list", {}, store)).toHaveLength(0);
});

test("Series approval navigates from another project to a newly created target, closes dialog and keeps failures visible", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/approval-navigation-ui-"));
  const previous = await call("project.save", { name: "Bisheriges Projekt" }, store);
  await call("job.submit", { project_id: previous.id, provider: "test", description: "Fremder Auftrag", idempotency_key: "previous" }, store);
  await routeCore(page, store);
  await page.goto("/");
  await expect(page.locator(".canvas-heading h2")).toHaveText(previous.name);
  // Simulates a project submitted by REST after the GUI loaded its project list.
  const target = await call("project.save", { name: "Zielprojekt aus REST" }, store);
  const request = await call("series.submit", { project_id: target.id, description: "Ein freigegebenes Kapitelbild", idempotency_key: "navigate-target", items: [{ provider: "test", description: "Zielauftrag der Serie" }] }, store);
  let approvals = 0;
  let showFailure = false;
  await page.route("**/rpc", async route => {
    const req = route.request().postDataJSON();
    if (req.op === "series.approve") approvals++;
    if (showFailure && req.op === "job.list" && req.args.project_id === target.id) {
      const jobs = await call(req.op, req.args, store);
      // Explicit failure display fixture, never a real provider request.
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(jobs.map((job: any) => ({ ...job, status: "failed", error: "Synthetischer Anbieterfehler HTTP 400", cost_status: "unknown", actual_microusd: null }))) });
    } else await route.fallback();
  });
  await page.getByRole("button", { name: "Serienfreigaben: 1", exact: true }).click();
  await page.getByRole("button", { name: "Gesamte Serie freigeben · 1 Bild", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".canvas-heading h2")).toHaveText(target.name);
  const drawer = page.getByRole("region", { name: "Aufträge", exact: true });
  await expect(drawer).toBeVisible();
  await expect(drawer.getByRole("heading", { name: `Aufträge · ${target.name}` })).toBeVisible();
  await expect(drawer).toContainText("Ein freigegebenes Kapitelbild");
  await expect(drawer).toContainText("Zielauftrag der Serie");
  await expect(drawer).not.toContainText("Fremder Auftrag");
  showFailure = true;
  await expect(drawer).toContainText("1 × Fehlgeschlagen");
  await expect(drawer).toContainText("Synthetischer Anbieterfehler HTTP 400");
  await page.screenshot({ path: path.join(proof, "35-series-project-handoff.png") });
  await page.setViewportSize({ width: 1000, height: 760 });
  await expect(drawer.getByRole("heading")).toBeVisible();
  await expect(drawer).toContainText("Synthetischer Anbieterfehler HTTP 400");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByRole("button", { name: "Serienfreigaben: 0", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Projekt und Aufträge öffnen", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(approvals).toBe(1);
  expect((await call("series.get", { id: request.id }, store)).jobs).toHaveLength(1);
});

test("Series navigation failure preserves approval and offers opening project without a second submit", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/approval-open-error-ui-"));
  const project = await call("project.save", { name: "Freigabe bleibt erhalten" }, store);
  await call("series.submit", { project_id: project.id, description: "Lokaler Testauftrag", idempotency_key: "open-failed", items: [{ provider: "test", description: "Ein Testbild" }] }, store);
  await routeCore(page, store);
  let failOpen = true;
  let approvals = 0;
  await page.route("**/rpc", async route => {
    const req = route.request().postDataJSON();
    if (req.op === "series.approve") approvals++;
    if (req.op === "project.get" && failOpen) {
      failOpen = false;
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: "Synthetischer Lesefehler" }) });
    } else await route.fallback();
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Serienfreigaben: 1", exact: true }).click();
  await page.getByRole("button", { name: "Gesamte Serie freigeben · 1 Bild", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("alert")).toContainText("Die Serie wurde freigegeben");
  await expect(dialog.getByRole("button", { name: /Gesamte Serie freigeben/ })).toHaveCount(0);
  await dialog.getByRole("button", { name: "Projekt und Aufträge öffnen", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Aufträge", exact: true })).toContainText("Ein Testbild");
  expect(approvals).toBe(1);
  expect(await call("job.list", {}, store)).toHaveLength(1);
});

test("Series model comparison: frozen quotes, key availability, quality and one-time budget approval", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/series-model-ui-"));
  await call("settings.save", {theme:"light", job_limit_microusd:10000}, store);
  const project = await call("project.save", {name:"Modellvergleich"}, store);
  const series = await call("series.submit", {project_id:project.id,description:"Drei Figuren vergleichen",idempotency_key:"compare-ui",items:[{description:"Figur frontal",provider:"gemini",model:"gemini-nano-banana-2.1",count:3}]}, store);
  await routeCore(page,store,false,["gemini","openai"]);
  // Synthetic readability only; this isolated store has no worker and never sends paid requests.
  await page.route("**/rpc", async route => {
    if (route.request().postDataJSON().op === "secret.check") return route.fulfill({status:200,contentType:"application/json",body:JSON.stringify({readable:true,provider_contacted:false})});
    await route.fallback();
  });
  await page.goto("/");
  await page.getByRole("button",{name:"Serienfreigaben: 1",exact:true}).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Kosten der Modelle werden lokal berechnet …")).toHaveCount(0);
  const low = dialog.getByRole("radio",{name:"GPT Image 2.5 Sunburst · 1K · low",exact:true});
  const medium = dialog.getByRole("radio",{name:"GPT Image 2.5 Sunburst · 1K · medium",exact:true});
  await expect(low).toBeEnabled();
  await low.check();
  const lowTotal = await dialog.locator(".series-total").innerText();
  await medium.check();
  await expect(dialog.locator(".series-total")).not.toHaveText(lowTotal);
  await expect(dialog.getByRole("radio",{name:/Testprovider/})).toHaveCount(0);
  await expect(dialog.getByRole("button",{name:/Gesamte Serie freigeben/})).toBeDisabled();
  await page.screenshot({path:path.join(proof,"36-series-model-comparison-light.png")});
  await page.evaluate(() => { document.documentElement.dataset.theme = "dark"; });
  await page.screenshot({path:path.join(proof,"37-series-model-comparison-dark.png")});
  await dialog.getByRole("checkbox",{name:/Einmalige Startgrenze/}).check();
  await dialog.getByRole("button",{name:/Gesamte Serie freigeben/}).click();
  await expect(dialog).toHaveCount(0);
  const result = await call("series.get",{id:series.id},store);
  expect(result.jobs).toHaveLength(3);
  expect(result.jobs.every((j:any) => j.snapshot.parameters.provider === "openai" && j.snapshot.parameters.quality === "medium")).toBe(true);
  expect(result.submitted_items[0].snapshot.parameters.provider).toBe("gemini");
  expect((await call("settings.get",{},store)).job_limit_microusd).toBe(10000);
  await call("series.cancel",{id:series.id},store);
});

test("Series model comparison: missing keys and incompatible aspect ratios stay disabled", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/series-incompatible-ui-"));
  const project = await call("project.save", {name:"Breitbild"}, store);
  await call("series.submit", {project_id:project.id,description:"Breite Figur",idempotency_key:"compare-wide",items:[{description:"Figur",provider:"gemini",model:"gemini-nano-banana-2.1",aspect_ratio:"16:9"}]}, store);
  await routeCore(page,store,false,["gemini"]);
  await page.goto("/");
  await page.getByRole("button",{name:"Serienfreigaben: 1",exact:true}).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("radio",{name:/GPT Image 2.5 Sunburst · 1K · medium/})).toBeDisabled();
  await expect(dialog).toContainText("Nicht unterstützter Modellparameter: aspect_ratio");
  await expect(dialog).toContainText("Kein Schlüssel hinterlegt");
  expect(await call("job.list",{},store)).toHaveLength(0);
});

test("Settings navigation: separate sections retain edits, save across sections and never submit typed keys implicitly", async ({page}) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/settings-navigation-"));
  await routeCore(page,store);
  let keyActions = 0;
  await page.route("**/rpc",async route => {
    if (route.request().postDataJSON().op.startsWith("secret.")) keyActions++;
    await route.fallback();
  });
  await page.goto("/");
  await page.getByRole("button",{name:"Einstellungen",exact:true}).click();
  const nav = page.getByRole("navigation",{name:"Einstellungsbereiche"});
  await expect(nav.getByRole("button")).toHaveCount(7);
  await expect(page.getByLabel("Gemini API-Schlüssel",{exact:true})).toBeVisible();
  await expect(page.getByLabel("Erscheinungsbild",{exact:true})).not.toBeVisible();
  await page.getByLabel("Gemini API-Schlüssel",{exact:true}).fill("synthetic-not-submitted");
  await page.getByLabel("Gemini API-Schlüssel",{exact:true}).press("Enter");
  expect(keyActions).toBe(0);
  await page.getByLabel("Gemini API-Schlüssel",{exact:true}).fill("");
  await page.screenshot({path:path.join(proof,"38-settings-keys-light.png")});
  await nav.getByRole("button",{name:"Darstellung",exact:true}).click();
  await page.getByLabel("Erscheinungsbild",{exact:true}).selectOption("dark");
  await nav.getByRole("button",{name:"Speicher & Export",exact:true}).click();
  await page.getByLabel("Namensschema",{exact:true}).fill("{projekt}_{datum}_{nummer:04}");
  await nav.getByRole("button",{name:"Budgets & Aufträge",exact:true}).click();
  await expect(page.getByText("Ungespeicherte Einstellungen.",{exact:false})).toBeVisible();
  await nav.getByRole("button",{name:"Speicher & Export",exact:true}).click();
  await expect(page.getByLabel("Namensschema",{exact:true})).toHaveValue("{projekt}_{datum}_{nummer:04}");
  await page.getByRole("button",{name:"Speichern",exact:true}).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const settings=await call("settings.get",{},store);
  expect(settings.theme).toBe("dark"); expect(settings.naming_template).toBe("{projekt}_{datum}_{nummer:04}");
  await page.getByRole("button",{name:"Einstellungen",exact:true}).click();
  await nav.getByRole("button",{name:"REST, MCP & Zugänge",exact:true}).click();
  await expect(page.getByRole("button",{name:"Neuen Zugang anlegen",exact:true})).toBeEnabled();
  await page.screenshot({path:path.join(proof,"39-settings-access-dark.png")});
  expect(keyActions).toBe(0);
});


test("External reference import refreshes the open project without a generation job or reload", async ({page}) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/external-reference-ui-"));
  const project = await call("project.save",{name:"Externe Referenz"},store);
  await routeCore(page,store);
  await page.goto("/");
  await expect(page.locator(".asset-card")).toHaveCount(0);
  const sourceProject = await call("project.save",{name:"Synthetic fixture"},store);
  await call("job.submit",{project_id:sourceProject.id,description:"Local reference fixture",provider:"test",idempotency_key:"source"},store);
  const localWorker=spawn(binary,["--data-dir",store,"--worker"],{stdio:["ignore","pipe","pipe"]});
  try {
    await expect.poll(async () => (await call("asset.list",{project_id:sourceProject.id},store)).total).toBe(1);
    const asset=(await call("asset.list",{project_id:sourceProject.id},store)).items[0];
    const image=await call("asset.read",{id:asset.id},store);
    await call("asset.import",{project_id:project.id,description:"Extern importierte Figur",data_base64:image.data_base64},store);
    await expect(page.locator(".asset-card")).toHaveCount(1);
    await expect(page.locator(".asset-card")).toContainText("Extern importierte Figur");
    expect(await call("job.list",{project_id:project.id},store)).toHaveLength(0);
  } finally { localWorker.kill("SIGTERM"); }
});


test("Series approval key failure stays visible at the clicked action without starting jobs", async ({page}) => {
  const store = await fs.mkdtemp(path.resolve(".artifacts/series-key-error-ui-"));
  await call("settings.save", {job_limit_microusd:70000}, store);
  const project = await call("project.save", {name:"Sichtbare Schlüsselfehler"}, store);
  const series = await call("series.submit", {project_id:project.id,description:"22 Ansichten",idempotency_key:"key-error",items:Array.from({length:22},(_,i)=>({description:`Ansicht ${i+1}`,provider:"openai",model:"gpt-image-2.5-sunburst-2026-09-08",quality:"medium"}))}, store);
  await routeCore(page,store,false,["gemini","openai"]);
  let checks = 0, approvals = 0;
  await page.route("**/rpc", async route => {
    const req = route.request().postDataJSON();
    if(req.op === "series.approve") approvals++;
    if(req.op === "secret.check") {
      checks++;
      expect(req.args.provider).toBe("openai");
      return route.fulfill({status:400,contentType:"application/json",body:JSON.stringify({error:"Synthetischer Schlüsselbundfehler (Code -25293)"})});
    }
    await route.fallback();
  });
  await page.setViewportSize({width:1100,height:760});
  await page.goto("/");
  await page.getByRole("button",{name:"Serienfreigaben: 1",exact:true}).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByRole("checkbox",{name:/Einmalige Startgrenze/}).check();
  const approve=dialog.getByRole("button",{name:/Gesamte Serie freigeben/});
  for(let attempt=0;attempt<2;attempt++) {
    await approve.click();
    const error=dialog.getByRole("alert");
    await expect(error).toContainText("Keine Freigabe erteilt, keine Bilder gestartet");
    await expect(error).toContainText("OpenAI");
    await expect(error).toContainText("-25293");
    await expect(error).toBeInViewport();
    await expect(dialog.locator(".series-action-feedback")).toBeFocused();
    await expect(approve).toBeEnabled();
  }
  expect(checks).toBe(2);
  expect(approvals).toBe(0);
  expect((await call("series.get",{id:series.id},store)).status).toBe("pending");
  expect(await call("job.list",{},store)).toHaveLength(0);
  await page.screenshot({path:path.join(proof,"40-series-key-error-visible.png")});
});

test("Generation progress counts persisted images and exposes work without invented provider percentages", async ({page}) => {
  const store=await fs.mkdtemp(path.resolve(".artifacts/generation-progress-ui-"));
  const project=await call("project.save",{name:"Fortschritt"},store);
  const series=await call("series.submit",{project_id:project.id,description:"22 Bilder Fortschritt",idempotency_key:"progress",items:Array.from({length:22},(_,i)=>({provider:"test",description:`Figur ${i+1}`}))},store);
  await call("series.approve",{id:series.id},store);
  await routeCore(page,store);
  let phase="running";
  await page.route("**/rpc", async route=>{
    const req=route.request().postDataJSON();
    if(req.op!=="job.list") return route.fallback();
    let jobs=await call("job.list",req.args,store);
    jobs=jobs.map((j:any,i:number)=>({...j,status:phase==="completed"?"completed":i<3?"completed":i===3?"failed":i===4?"running":"queued"}));
    if(phase==="single") jobs=[{...jobs[0],series_id:null,idempotency_key:"single:0",status:"running",description:"Einzelbild in Bearbeitung"}];
    return route.fulfill({status:200,contentType:"application/json",body:JSON.stringify(jobs)});
  });
  await page.goto("/");
  const region=page.getByRole("region",{name:"Bildgenerierung – Fortschritt",exact:true});
  await expect(region).toContainText("3 / 22 Bilder fertig");
  await expect(region).toContainText("1 in Bearbeitung · 17 warten · 1 ohne fertiges Bild");
  await expect(region.getByRole("progressbar")).toHaveAttribute("value","3");
  await expect(region.getByRole("progressbar")).toHaveAttribute("max","22");
  await expect(region).toContainText("Der Anbieter meldet keinen Prozentfortschritt");
  await region.getByRole("button",{name:"Aufträge ansehen"}).click();
  await expect(page.getByRole("region",{name:"Aufträge",exact:true})).toBeVisible();
  await page.getByRole("region",{name:"Aufträge",exact:true}).getByRole("button",{name:"Schließen",exact:true}).click();
  await page.screenshot({path:path.join(proof,"41-generation-progress-light.png")});
  await page.evaluate(()=>{document.documentElement.dataset.theme="dark";});
  await page.screenshot({path:path.join(proof,"42-generation-progress-dark.png")});
  phase="completed";
  await expect(region).toContainText("22 / 22 Bilder fertig");
  await expect(region.getByRole("progressbar")).toHaveAttribute("value","22");
  await expect(region.locator(".generation-spinner")).toHaveCount(0);
  phase="single";
  await expect(region).toContainText("Einzelbild in Bearbeitung");
  await expect(region.getByRole("progressbar")).not.toHaveAttribute("value");
  await page.emulateMedia({reducedMotion:"reduce"});
  expect(await region.locator(".generation-spinner").evaluate(el=>getComputedStyle(el).animationName)).toBe("none");
  await call("series.cancel",{id:series.id},store);
});

test("Export whole series ignores gallery filters and unrelated project assets", async ({page})=>{
  const store=await fs.mkdtemp(path.resolve(".artifacts/series-export-ui-"));
  const project=await call("project.save",{name:"Serienexport"},store);
  const series=await call("series.submit",{project_id:project.id,description:"Drei Serienbilder",idempotency_key:"export-all",items:[{provider:"test",description:"Serie",count:3}]},store);
  await call("series.approve",{id:series.id},store);
  const w=spawn(binary,["--data-dir",store,"--worker"],{stdio:["ignore","pipe","pipe"]});
  try {
    await expect.poll(async()=>(await call("series.get",{id:series.id},store)).job_status_counts.completed).toBe(3);
    const ordered=(await call("asset.list",{project_id:project.id,series_id:series.id},store)).items;
    const reference=await fs.readFile(path.join(store,ordered[0].path));
    await call("asset.import",{project_id:project.id,name:"reference.png",data_base64:reference.toString("base64")},store);
    await routeCore(page,store);
    let submitted:any, exportJobId = "";
    await page.route("**/rpc",async route=>{const req=route.request().postDataJSON();if(req.op==="export.submit") submitted=req.args;if(req.op==="export.get") exportJobId=req.args.id;await route.fallback();});
    await page.goto("/");
    await page.getByRole("textbox",{name:"Beschreibung oder Tags suchen"}).fill("nothing-matches");
    await page.getByRole("button",{name:"Serie exportieren",exact:true}).click();
    const dialog=page.getByRole("dialog");
    await expect(dialog).toContainText("Erzeugte Bilder dieser Serie · 3");
    await dialog.getByRole("combobox",{name:"Exportformat"}).selectOption("original");
    await dialog.getByRole("button",{name:"Exportieren",exact:true}).click();
    await expect(dialog).toContainText("Export erstellt");
    expect(submitted.asset_ids).toEqual(ordered.map((a:any)=>a.id));
    const output=(await call("export.get",{id:exportJobId},store)).result;
    expect(output.image_count).toBe(3);
    expect(output.files).toHaveLength(3);
    for(let i=0;i<3;i++) expect(await fs.readFile(path.join(output.path,output.files[i]))).toEqual(await fs.readFile(path.join(store,ordered[i].path)));
    await page.screenshot({path:path.join(proof,"43-series-export.png")});
    await dialog.getByRole("button",{name:"Schließen",exact:true}).click();
    await page.getByRole("button",{name:"Serienfreigaben: 0",exact:true}).click();
    await page.getByRole("dialog").getByRole("button",{name:"Serie exportieren",exact:true}).click();
    await expect(page.getByRole("dialog")).toContainText("Erzeugte Bilder dieser Serie · 3");
    await expect(page.getByRole("dialog")).toHaveCount(1);
  } finally {w.kill("SIGTERM");}
});

test("Structured prompt editor separates scene character and instruction and refreshes shared definitions",async({page})=>{
  const store=await fs.mkdtemp(path.resolve(".artifacts/structured-prompt-ui-"));
  const project=await call("project.save",{name:"Raumschiff",description:"Intro für ein Bootcamp"},store);
  const character=await call("character.save",{project_id:project.id,name:"Pilot",description:"Brauner Vollbart",clothing:"Grüner Anzug",reference_ids:[],approved:false},store);
  await routeCore(page,store);
  await page.goto("/");
  const generator=page.getByRole("complementary",{name:"Generierung"});
  await expect(generator.getByRole("group",{name:"1 · Gesamte Szene",exact:true})).toBeVisible();
  const navigation=generator.getByRole("navigation",{name:"Promptbereich auswählen"});
  await navigation.getByRole("button",{name:"Einzelanweisung",exact:true}).click();
  await expect(generator.getByRole("group",{name:"3 · Einzelanweisung",exact:true})).toBeFocused();
  await navigation.getByRole("button",{name:"Charakter",exact:true}).click();
  await expect(generator.getByRole("group",{name:"2 · Charakter & Referenzen",exact:true})).toBeFocused();
  await navigation.getByRole("button",{name:"Szene",exact:true}).click();
  await expect(generator.getByRole("group",{name:"1 · Gesamte Szene",exact:true})).toBeFocused();
  await generator.getByLabel("Szenenbeschreibung",{exact:true}).fill("LEGO-Raumschiff in einem hellen Hangar");
  await generator.getByLabel("Charakter",{exact:true}).selectOption(character.id);
  await generator.getByLabel("Zusätzliche Charakterbeschreibung",{exact:true}).fill("Zweiter Pilot mit Brille und blauem Hemd");
  await generator.getByLabel("Einzelanweisung",{exact:true}).fill("Beide Piloten winken im Cockpit");
  const preview=generator.locator(".prompt-preview pre");
  await expect(preview).toContainText("[Gesamte Szene]");
  await expect(preview).toContainText("LEGO-Raumschiff in einem hellen Hangar");
  await expect(preview).toContainText("Brauner Vollbart");
  await expect(preview).toContainText("Zweiter Pilot mit Brille und blauem Hemd");
  await expect(preview).toContainText("[Einzelanweisung]");
  await expect(preview).toContainText("Beide Piloten winken im Cockpit");
  await generator.getByRole("button",{name:"Gespeicherten Charakter bearbeiten",exact:true}).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByLabel("Beschreibung",{exact:true}).fill("Vollbart und braunes Haarteil");
  await dialog.getByRole("button",{name:"Speichern",exact:true}).click();
  await expect(dialog).toHaveCount(0);
  await expect(preview).toContainText("Vollbart und braunes Haarteil");
  await expect(generator.getByLabel("Einzelanweisung",{exact:true})).toHaveValue("Beide Piloten winken im Cockpit");
  await generator.getByText("Projektbeschreibung · wird mitgesendet",{exact:true}).click();
  await generator.getByRole("button",{name:"Projektbeschreibung bearbeiten",exact:true}).click();
  await page.getByRole("dialog").getByLabel("Beschreibung",{exact:true}).fill("Neuer Bootcamp-Kontext");
  await page.getByRole("dialog").getByRole("button",{name:"Speichern",exact:true}).click();
  await expect(preview).toContainText("Neuer Bootcamp-Kontext");
  await expect(generator.getByLabel("Szenenbeschreibung",{exact:true})).toHaveValue("LEGO-Raumschiff in einem hellen Hangar");
  await generator.getByRole("button",{name:"Experte",exact:true}).click();
  await expect(generator.getByLabel("Einzelanweisung",{exact:true})).toHaveValue("Beide Piloten winken im Cockpit");
  await generator.getByRole("button",{name:"Einfach",exact:true}).click();
  await generator.getByRole("button",{name:"1 Bild erzeugen",exact:true}).click();
  await expect.poll(async()=>(await call("job.list",{project_id:project.id},store)).length).toBe(1);
  const job=(await call("job.list",{project_id:project.id},store))[0];
  expect(job.snapshot.parameters.description).toBe("Beide Piloten winken im Cockpit");
  expect(job.snapshot.blocks.scene).toBe("LEGO-Raumschiff in einem hellen Hangar");
  expect(job.snapshot.blocks.character).toBe("Zweiter Pilot mit Brille und blauem Hemd");
  expect(job.snapshot.character.description).toBe("Vollbart und braunes Haarteil");
  await call("job.cancel",{id:job.id},store);
  await page.getByRole("region",{name:"Aufträge",exact:true}).getByRole("button",{name:"Schließen",exact:true}).click();
  await generator.getByLabel("Szenenbeschreibung",{exact:true}).scrollIntoViewIfNeeded();
  await page.screenshot({path:path.join(proof,"44-structured-prompt-light.png")});
  await page.evaluate(()=>{document.documentElement.dataset.theme="dark";});
  await page.setViewportSize({width:1100,height:760});
  await generator.getByLabel("Zusätzliche Charakterbeschreibung",{exact:true}).scrollIntoViewIfNeeded();
  await page.screenshot({path:path.join(proof,"45-structured-prompt-dark.png")});
});

test("Workshop v020: three section AI editing, template persistence, trash restore and SBOM",async({page})=>{
 const store=await fs.mkdtemp(path.resolve('.artifacts/workshop-ui-'));
 const project=await call('project.save',{name:'Drei Bereiche',prompt_template:{scene:'Hangar',character:'Roboter',instruction:'Winken'}},store);
 const w=spawn(binary,['--data-dir',store,'--worker'],{stdio:'ignore'});
 try {
 await routeCore(page,store);await page.goto('/');await page.setViewportSize({width:1100,height:760});
 const g=page.getByRole('complementary',{name:'Generierung'});
 await expect(g.getByLabel('Szenenbeschreibung',{exact:true})).toHaveValue('Hangar');
 await g.getByRole('button',{name:'Mit KI in drei Bereiche aufteilen / verbessern',exact:true}).click();
 const d=page.getByRole('dialog');
 await d.getByLabel('Prompt-Assistent wählen',{exact:true}).selectOption('test');
 await d.getByRole('button',{name:'Vorschlag anfordern',exact:true}).click();
 await expect(d.getByRole('button',{name:'Drei Bereiche übernehmen',exact:true})).toBeEnabled();
 const results=d.locator('.assistant-result textarea');await expect(results).toHaveCount(3);
 await results.nth(0).fill('Heller Hangar');await results.nth(1).fill('Roboter mit Brille');await results.nth(2).fill('Mit linker Hand winken');
 await d.getByRole('button',{name:'Drei Bereiche übernehmen',exact:true}).click();
 await expect(g.getByLabel('Einzelanweisung',{exact:true})).toHaveValue('Mit linker Hand winken');
 await g.getByRole('button',{name:'Szene mit KI verbessern',exact:true}).click();
 await expect(d.getByLabel('Beschreibung, die gesendet wird',{exact:true})).toHaveValue('Heller Hangar');
 await d.getByLabel('Prompt-Assistent wählen',{exact:true}).selectOption('test');await d.getByRole('button',{name:'Vorschlag anfordern',exact:true}).click();
 await expect(d.locator('.assistant-result textarea')).toBeVisible();await d.locator('.assistant-result textarea').fill('Hangar bei Nacht');
 await d.getByRole('button',{name:'Szenenbeschreibung übernehmen',exact:true}).click();
 await expect(g.getByLabel('Zusätzliche Charakterbeschreibung',{exact:true})).toHaveValue('Roboter mit Brille');
 await g.getByRole('button',{name:'Drei Bereiche im Projekt speichern',exact:true}).click();
 await expect(g.getByRole('status')).toContainText('Promptvorlage im Projekt gespeichert.');
 await page.reload();await expect(g.getByLabel('Szenenbeschreibung',{exact:true})).toHaveValue('Hangar bei Nacht');
 await expect(g.getByLabel('Einzelanweisung',{exact:true})).toHaveValue('Mit linker Hand winken');
 await page.getByRole('button',{name:/^Projekte/}).first().click();
 await page.getByRole('button',{name:'Projekt löschen: Drei Bereiche',exact:true}).click();
 await d.getByRole('button',{name:'In den Papierkorb verschieben',exact:true}).click();
 await expect(page.getByRole('button',{name:'Projekt öffnen: Drei Bereiche',exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'Papierkorb',exact:true}).click();
 await d.getByRole('button',{name:'Projekt wiederherstellen',exact:true}).click();
 await d.getByRole('button',{name:'Schließen',exact:true}).click();
 await expect(page.getByRole('button',{name:'Projekt öffnen: Drei Bereiche',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Einstellungen',exact:true}).click();
 await d.getByRole('button',{name:'Über FrameStudio & SBOM',exact:true}).click();
 const download=page.waitForEvent('download');await d.getByRole('button',{name:'SBOM als JSON exportieren',exact:true}).click();
 const file=await download;const data=JSON.parse(await fs.readFile((await file.path())!,'utf8'));expect(data.bomFormat).toBe('CycloneDX');expect(data.components.length).toBeGreaterThan(600);
 await page.screenshot({path:path.join(proof,'46-sbom.png')});
 expect((await call('project.get',{id:project.id},store)).prompt_template.scene).toBe('Hangar bei Nacht');
 }finally{w.kill('SIGTERM');}
});

test("Workshop v020: small-window style actions and immediate preview refresh",async({page})=>{
 const store=await fs.mkdtemp(path.resolve('.artifacts/workshop-preview-ui-'));
 const w=spawn(binary,['--data-dir',store,'--worker'],{stdio:'ignore'});
 try{await routeCore(page,store);await page.goto('/');await page.setViewportSize({width:1000,height:720});
 await page.getByRole('button',{name:/^Stilbibliothek/}).click();
 const card=page.locator('.library-card').filter({has:page.getByRole('heading',{name:'Comic mit klarer Kontur',exact:true})});
 await card.getByRole('button',{name:'Vorschau gestalten',exact:true}).click();
 const d=page.getByRole('dialog');const generate=d.getByRole('button',{name:'1 Bild erzeugen',exact:true});
 await expect(generate).toBeInViewport();const box=await generate.boundingBox();expect(box!.y+box!.height).toBeLessThanOrEqual(720);
 await page.screenshot({path:path.join(proof,'47-style-actions-small.png')});await generate.click();
 await expect(d.getByRole('button',{name:'Als Stilvorschau übernehmen',exact:true})).toBeVisible();
 await d.getByRole('button',{name:'Schließen',exact:true}).click();
 await expect(card.getByRole('img',{name:'Stilvorschau: Comic mit klarer Kontur',exact:true})).toBeVisible();
 await expect(card).toContainText('noch nicht übernommen');await page.screenshot({path:path.join(proof,'48-live-style-library.png')});
 }finally{w.kill('SIGTERM');}
});

test("Workshop v020: import a character reference and choose generated portrait independently",async({page})=>{
 const store=await fs.mkdtemp(path.resolve('.artifacts/workshop-character-ui-'));
 const p=await call('project.save',{name:'Charakterwerkstatt'},store);
 const j=await call('job.submit',{project_id:p.id,provider:'test',model:'deterministic-v1',description:'Erzeugtes Charakterbild',idempotency_key:'portrait',count:1},store);
 const w=spawn(binary,['--data-dir',store,'--worker'],{stdio:'ignore'});
 try{await expect.poll(async()=>(await call('job.get',{id:j.jobs[0].id},store)).status).toBe('completed');
 const assets=(await call('asset.list',{project_id:p.id},store)).items;
 await routeCore(page,store);await page.goto('/');await page.setViewportSize({width:1000,height:720});
 await page.getByRole('button',{name:/^Charaktere/}).click();await page.getByRole('button',{name:'Neuer Charakter',exact:true}).click();
 const d=page.getByRole('dialog');await expect(d.getByRole('button',{name:'Speichern',exact:true})).toBeInViewport();
 await d.getByLabel('Name',{exact:true}).fill('Pilot');
 const input=d.locator('input[type=file]');await input.setInputFiles({name:'Referenzfoto.png',mimeType:'image/png',buffer:await fs.readFile(path.join(store,assets[0].path))});
 await expect(d.getByLabel('Bilderauswahl',{exact:true})).toHaveValue('import');
 await expect(d.getByRole('button',{name:/Referenzfoto.png/})).toHaveAttribute('aria-pressed','true');
 await d.getByLabel('Charakterbild für die Bibliothek',{exact:true}).selectOption(assets[0].id);
 await page.screenshot({path:path.join(proof,'49-character-reference-portrait.png')});
 await d.getByRole('button',{name:'Speichern',exact:true}).click();
 const c=(await call('character.list',{project_id:p.id},store))[0];expect(c.reference_ids).toHaveLength(1);expect(c.reference_ids[0]).not.toBe(c.portrait_asset_id);expect(c.portrait_asset_id).toBe(assets[0].id);
 await expect(page.getByRole('img',{name:'Pilot',exact:true})).toBeVisible();
 const expected=await call('asset.read',{id:assets[0].id,thumbnail:true},store);
 await expect(page.getByRole('img',{name:'Pilot',exact:true})).toHaveAttribute('src',`data:${expected.mime};base64,${expected.data_base64}`);
 }finally{w.kill('SIGTERM');}
});

test("Workshop v020: dialog actions remain reachable at minimum window size",async({page})=>{
 const store=await fs.mkdtemp(path.resolve('.artifacts/workshop-dialogs-ui-'));
 const p=await call('project.save',{name:'Dialogprüfung'},store);
 await routeCore(page,store);await page.goto('/');await page.setViewportSize({width:1000,height:720});
 async function actionVisible(name:string){const d=page.getByRole('dialog');await expect(d.getByRole('button',{name,exact:true}).last()).toBeInViewport();await expect(d.getByRole('button',{name:'Schließen',exact:true}).first()).toBeInViewport();await d.getByRole('button',{name:'Schließen',exact:true}).first().click();}
 await page.getByRole('button',{name:'Neues Projekt',exact:true}).first().click();await actionVisible('Speichern');
 await page.getByRole('button',{name:/^Stilbibliothek/}).click();await page.getByRole('button',{name:'Neuer Stil',exact:true}).click();await actionVisible('Speichern');
 await page.getByRole('button',{name:'Dialogprüfung',exact:true}).click();
 await page.getByRole('button',{name:'Charakterset planen',exact:true}).click();await actionVisible('Liste übernehmen');
 await page.getByRole('button',{name:'Referenzen',exact:true}).click();await actionVisible('Schließen');
 await page.getByRole('button',{name:'Einstellungen',exact:true}).click();
 for(const section of ['API-Schlüssel','Preise & Anbieter','REST, MCP & Zugänge','Darstellung','Speicher & Export','Budgets & Aufträge','Über FrameStudio & SBOM']){
 await page.getByRole('navigation',{name:'Einstellungsbereiche'}).getByRole('button',{name:section,exact:true}).click();
 await expect(page.getByRole('dialog').getByRole('button',{name:'Speichern',exact:true})).toBeInViewport();
 }
 await actionVisible('Speichern');
 const data=await page.evaluate(()=>{const c=document.createElement('canvas');c.width=64;c.height=64;return c.toDataURL('image/png').split(',')[1];});
 await call('asset.import',{project_id:p.id,name:'dialog.png',description:'Dialogbild',data_base64:data},store);
 await page.reload();await page.getByRole('button',{name:'Exportieren',exact:true}).click();await actionVisible('Exportieren');
 await page.getByRole('button',{name:'Bilddetails',exact:true}).click();await actionVisible('Speichern');
 await page.screenshot({path:path.join(proof,'50-dialog-audit.png')});expect((await call('job.list',{project_id:p.id},store)).length).toBe(0);
});

test("beginner single image: shared core, one image, English persistence and German return", async ({ page }) => {
  const store = await fs.mkdtemp(path.resolve('.artifacts/beginner-ui-'));
  const beginnerWorker=spawn(binary,['--data-dir',store,'--worker'],{stdio:'ignore'});
  try {
  await routeCore(page, store);
  await page.goto('/');
  await page.getByRole('button',{name:'Einzelbild erzeugen',exact:true}).click();
  const generator=page.getByRole('complementary',{name:'Generierung'});
  await expect(generator.getByRole('heading',{name:'Einzelbild erzeugen'})).toBeVisible();
  await expect(generator.getByLabel('Beschreibung',{exact:true})).toBeVisible();
  await expect(generator.getByLabel('Szenenbeschreibung',{exact:true})).toBeHidden();
  await generator.getByLabel('Beschreibung',{exact:true}).fill('Synthetischer Würfel mit runden Ecken');
  await expect(generator.getByLabel('Bildanzahl',{exact:true})).toHaveValue('1');
  await expect(generator.getByLabel('Bildanzahl',{exact:true})).toBeDisabled();
  await generator.getByRole('button',{name:'1 Bild erzeugen',exact:true}).click();
  const projects=await call('project.list',{},store);
  const quick=projects.find((p:any)=>p.preferences?.purpose==='single_images');
  expect(Boolean(quick)).toBe(true);
  const jobs=await call('job.list',{project_id:quick.id},store);
  expect(jobs.length).toBe(1);
  expect(jobs[0].snapshot.parameters.provider).toBe('test');
  await expect.poll(async()=>(await call('job.get',{id:jobs[0].id},store)).status).toBe('completed');
  await expect(page.locator('.asset-card')).toHaveCount(1);
  await page.getByRole('button',{name:'Einstellungen',exact:true}).click();
  await page.getByRole('button',{name:'Darstellung',exact:true}).click();
  await page.getByLabel('Sprache',{exact:true}).selectOption('en');
  await page.getByRole('button',{name:'Speichern',exact:true}).click();
  await page.reload();
  await expect(page.getByRole('button',{name:'Create single image',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByRole('button',{name:'Appearance',exact:true}).click();
  await expect(page.getByLabel('Language',{exact:true})).toHaveValue('en');
  await page.getByRole('button',{name:'Storage & export',exact:true}).click();
  await expect(page.getByRole('button',{name:'Switch workspace on next start'})).toBeVisible();
  await page.getByRole('button',{name:'API keys',exact:true}).click();
  await page.getByText('Where do I get an API key?',{exact:true}).click();
  await expect(page.getByRole('button',{name:'Open API key page'})).toBeVisible();
  await page.getByRole('button',{name:'REST, MCP & access',exact:true}).click();
  await expect(page.getByRole('button',{name:'Create access grant'})).toBeVisible();
  await page.screenshot({path:path.join(proof,'51-english-settings.png')});
  await page.getByRole('button',{name:'Appearance',exact:true}).click();
  await page.getByLabel('Language',{exact:true}).selectOption('de');
  await page.getByRole('button',{name:'Save',exact:true}).click();
  await expect(page.getByRole('button',{name:'Einzelbild erzeugen',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Einzelbild erzeugen',exact:true}).click();
  expect((await call('project.list',{},store)).filter((p:any)=>p.preferences?.purpose==='single_images').length).toBe(1);
  await page.screenshot({path:path.join(proof,'52-single-image.png')});
  } finally { beginnerWorker.kill('SIGTERM'); }
});
