import { test, expect, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { createInterface } from "node:readline";

test.use({ trace: "off", screenshot: "off", video: "off" });
const binary = path.resolve("src-tauri/target/debug/framestudio");
let store: string;
function call(op: string, args: Record<string, unknown> = {}) {
  return new Promise<any>((resolve, reject) => {
    const child = spawn(binary, ["--data-dir", store, "--rpc"], { stdio: ["pipe", "pipe", "pipe"] });
    let output = "";
    let error = "";
    child.stdout.on("data", (data) => { output += data; });
    child.stderr.on("data", (data) => { error += data; });
    child.on("close", (code) => {
      if (code !== 0) return reject(new Error(error.replace(/^Error:\s*/, "").trim()));
      try { resolve(JSON.parse(output)); } catch { reject(new Error("Invalid local response")); }
    });
    child.stdin.end(JSON.stringify({ op, args }));
  });
}
async function routeCore(page: Page, delayedFailure?: () => Promise<void>) {
  await page.route("**/rpc", async (route) => {
    const request = route.request().postDataJSON();
    if (request.op === "secret.set" && delayedFailure) {
      await delayedFailure();
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: "Synthetischer UI-Test: Schlüsselspeicher antwortet nicht rechtzeitig." }) });
      return;
    }
    // Only a deliberately invalid short input is ever forwarded; no native writes are permitted.
    if (request.op.startsWith("secret.") && !(request.op === "secret.set" && request.args.key === "short")) {
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: "Native Schlüsseländerungen sind in diesem UI-Test gesperrt." }) });
      return;
    }
    try {
      const result = await call(request.op, request.args);
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(result) });
    } catch (cause) {
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: cause instanceof Error ? cause.message : String(cause) }) });
    }
  });
}
test.beforeAll(async () => {
  await fs.mkdir(path.resolve(".artifacts"), { recursive: true });
  store = await fs.mkdtemp(path.resolve(".artifacts/keychain-ui-"));
  await call("settings.save", { theme: "light" });
});
test.afterEach(async ({ page }) => { await page.close(); });

test("Key input: actual core rejects malformed input before OS access and shows error next to fields", async ({ page }) => {
  await routeCore(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  const region = page.getByRole("region", { name: "Schlüsselverwaltung", exact: true });
  await region.getByLabel("Gemini API-Schlüssel", { exact: true }).fill("short");
  await region.getByRole("button", { name: "Schlüssel sicher speichern", exact: true }).click();
  await expect(region.getByRole("alert")).toContainText("Ungültiges Schlüsselformat");
  expect((await region.getByLabel("Gemini API-Schlüssel", { exact: true }).inputValue()).length === 0).toBe(true);
  expect((await region.getByRole("alert").innerText()).includes("short")).toBe(false);
  await expect(region.getByRole("status")).toHaveCount(0);
  await expect(region.getByText("FrameStudio öffnet keine Schlüsselbund-Dialoge automatisch.", { exact: false })).toBeVisible();
  await region.getByRole("alert").scrollIntoViewIfNeeded();
  await fs.mkdir(path.resolve(".artifacts/ui-proof"), { recursive: true });
  await page.screenshot({ path: path.resolve(".artifacts/ui-proof/25-key-error-local.png") });
});

test("Key UI only: delayed rejection keeps busy state, prevents duplicate submit and never claims native success", async ({ page }) => {
  let release: () => void = () => {};
  let pending = new Promise<void>((resolve) => { release = resolve; });
  let submitted = 0;
  await routeCore(page, async () => { submitted += 1; await pending; });
  await page.goto("/");
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  const region = page.getByRole("region", { name: "Schlüsselverwaltung", exact: true });
  await region.getByLabel("Gemini API-Schlüssel", { exact: true }).fill("synthetic-not-a-provider-key");
  await region.getByRole("button", { name: "Schlüssel sicher speichern", exact: true }).evaluate((node) => { (node as HTMLButtonElement).click(); (node as HTMLButtonElement).click(); });
  await expect.poll(() => submitted).toBe(1);
  await expect(region.getByRole("status")).toContainText("Schlüsselspeicher wird angesprochen");
  await expect(region.getByRole("button", { name: "Schlüssel sicher speichern", exact: true })).toBeDisabled();
  await expect(region.getByLabel("Anbieter für Schlüsselablage", { exact: true })).toBeDisabled();
  expect((await region.getByLabel("Gemini API-Schlüssel", { exact: true }).inputValue()).length === 0).toBe(true);
  release();
  await expect(region.getByRole("alert")).toContainText("Synthetischer UI-Test");
  await expect(region.getByRole("status")).toHaveCount(0);
  await expect(region.getByLabel("Anbieter für Schlüsselablage", { exact: true })).toBeEnabled();
  expect(submitted).toBe(1);
  pending = new Promise<void>((resolve) => { release = resolve; });
  await region.getByLabel("Gemini API-Schlüssel", { exact: true }).fill("synthetic-not-a-provider-key");
  await region.getByRole("button", { name: "Schlüssel sicher speichern", exact: true }).click();
  await expect.poll(() => submitted).toBe(2);
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  release();
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await expect(region.getByRole("alert")).toHaveCount(0);
  await expect(region.getByRole("status")).toHaveCount(0);
  expect((await region.getByLabel("Gemini API-Schlüssel", { exact: true }).inputValue()).length === 0).toBe(true);
});

test("Session credentials: actual persistent Rust core, provider isolation, remove, window reload and process restart", async ({ page }) => {
  const directory = await fs.mkdtemp(path.resolve(".artifacts/session-ui-"));
  const fixture = path.resolve("src-tauri/target/debug/examples/session_ui_fixture");
  function start() {
    const child = spawn(fixture, [directory], { stdio: ["pipe", "pipe", "pipe"] });
    const pending: Array<{ resolve: (value: any) => void; reject: (error: Error) => void }> = [];
    createInterface({ input: child.stdout }).on("line", (line) => {
      const next = pending.shift();
      if (!next) return;
      try {
        const reply = JSON.parse(line);
        if (reply.error) next.reject(new Error(reply.error));
        else next.resolve(reply.value);
      } catch { next.reject(new Error("Invalid fixture response")); }
    });
    let ended = false;
    function finish() {
      ended = true;
      for (const next of pending.splice(0)) next.reject(new Error("Fixture stopped or could not start"));
    }
    child.on("exit", finish);
    child.on("error", finish);
    return {
      child,
      call: (op: string, args: Record<string, unknown> = {}) => new Promise<any>((resolve, reject) => {
        if (ended) { reject(new Error("Fixture is not running")); return; }
        pending.push({ resolve, reject });
        child.stdin.write(JSON.stringify({ op, args }) + "\n");
      }),
      stop: () => new Promise<void>((resolve) => {
        if (ended) { resolve(); return; }
        child.once("exit", () => resolve()); child.once("error", () => resolve()); child.stdin.end();
      }),
    };
  }
  let core = start();
  await page.route("**/rpc", async (route) => {
    try {
      const req = route.request().postDataJSON();
      const value = await core.call(req.op, req.args);
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(value) });
    } catch {
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: "Session fixture rejected operation" }) });
    }
  });
  try {
    await page.goto("/");
    await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
    const region = page.getByRole("region", { name: "Schlüsselverwaltung", exact: true });
    await region.getByLabel("Schlüssel verwenden", { exact: true }).selectOption("session");
    await region.getByLabel("Gemini API-Schlüssel", { exact: true }).fill("synthetic-session-browser-only-9811");
    await region.getByRole("button", { name: "Für diese Sitzung verwenden", exact: true }).click();
    await expect(region.getByRole("status")).toHaveText("Sitzungsschlüssel aktiv · nicht gespeichert");
    expect((await region.getByLabel("Gemini API-Schlüssel", { exact: true }).inputValue()).length === 0).toBe(true);
    await region.getByRole("button", { name: "Schlüsselzugriff prüfen", exact: true }).click();
    await expect(region.getByRole("status")).toContainText("Schlüssel ist für diese App lesbar");
    let presence = await core.call("secret.session.status");
    expect(presence.providers).toEqual({ gemini: true, openai: false, anthropic: false });
    await region.getByRole("status").scrollIntoViewIfNeeded();
    await page.screenshot({ path: path.resolve(".artifacts/ui-proof/26-session-key-active.png") });
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await page.reload();
    await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
    await expect(region.getByText("Google Gemini: Sitzungsschlüssel aktiv · nicht gespeichert", { exact: false })).toBeVisible();
    await region.getByLabel("Schlüssel verwenden", { exact: true }).selectOption("session");
    await region.getByRole("button", { name: "Sitzungsschlüssel entfernen", exact: true }).click();
    await expect(region.getByRole("status")).toContainText("Sitzungsschlüssel entfernt.");
    presence = await core.call("secret.session.status");
    expect(presence.providers.gemini).toBe(false);
    await region.getByLabel("Anbieter für Schlüsselablage", { exact: true }).selectOption("openai");
    await region.getByLabel("OpenAI API-Schlüssel", { exact: true }).fill("synthetic-session-browser-only-9812");
    await region.getByRole("button", { name: "Für diese Sitzung verwenden", exact: true }).click();
    await expect(region.getByRole("status")).toContainText("Sitzungsschlüssel aktiv");
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await core.stop();
    core = start();
    presence = await core.call("secret.session.status");
    expect(presence.providers).toEqual({ gemini: false, openai: false, anthropic: false });
    await page.reload();
    await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
    await expect(region.getByText("Sitzungsschlüssel aktiv", { exact: false })).toHaveCount(0);
  } finally {
    await page.close();
    await core.stop();
  }
});

test("Explicit keychain check requests OS storage even while a session credential exists", async ({page}) => {
  let checkedSource = "";
  await routeCore(page);
  await page.route("**/rpc", async route => {
    const req=route.request().postDataJSON();
    if(req.op === "secret.check") {
      checkedSource=req.args.source;
      return route.fulfill({status:400,contentType:"application/json",body:JSON.stringify({error:"Synthetischer Schlüsselbund-Test: Code -25293"})});
    }
    await route.fallback();
  });
  await page.goto("/");
  await page.getByRole("button",{name:"Einstellungen",exact:true}).click();
  await page.getByRole("button",{name:"Nur Schlüsselbund prüfen",exact:true}).click();
  expect(checkedSource).toBe("keychain");
  await expect(page.locator(".settings-key-summary")).toContainText("Google Gemini: Schlüsselbundzugriff fehlgeschlagen");
  await expect(page.getByRole("region",{name:"Schlüsselverwaltung",exact:true}).getByRole("alert")).toContainText("-25293");
});
