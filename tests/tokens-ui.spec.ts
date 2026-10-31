import { test, expect, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";

// Never record a generated credential in browser traces, videos, screenshots or assertion output.
test.use({ trace: "off", screenshot: "off", video: "off" });
const binary = path.resolve("src-tauri/target/debug/framestudio");
let store: string;
function call(op: string, args: Record<string, unknown> = {}) {
  return new Promise<any>((resolve, reject) => {
    const child = spawn(binary, ["--data-dir", store, "--rpc"], { stdio: ["pipe", "pipe", "pipe"] });
    let output = "";
    child.stdout.on("data", (data) => { output += data; });
    child.stderr.on("data", () => {});
    child.on("close", (code) => {
      if (code !== 0) return reject(new Error(`Local operation failed: ${op}`));
      try { resolve(JSON.parse(output)); } catch { reject(new Error("Invalid local response")); }
    });
    child.stdin.end(JSON.stringify({ op, args }));
  });
}
async function routeCore(page: Page) {
  await page.route("**/rpc", async (route) => {
    const request = route.request().postDataJSON();
    try {
      const result = await call(request.op, request.args);
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(result) });
    } catch {
      await route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ error: "Lokaler Testauftrag fehlgeschlagen." }) });
    }
  });
}
test.beforeAll(async () => {
  await fs.mkdir(path.resolve(".artifacts"), { recursive: true });
  store = await fs.mkdtemp(path.resolve(".artifacts/token-ui-"));
  await call("settings.save", { theme: "light" });
});

test.afterEach(async ({ page }) => {
  // Close before failure-context capture; no raw value may survive in diagnostic DOM snapshots.
  await page.close();
  await call("tokens.revoke");
});

test("Token access: once-only copy, durable recipient and dates, edit and independent revocation", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await routeCore(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"REST, MCP & Zugänge",exact:true}).click();
  await expect(page.getByText("Noch keine Zugänge.", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "Neuen Zugang anlegen", exact: true }).click();
  await page.getByLabel("Name des Zugangs", { exact: true }).fill("Synthetischer Präsentationshelfer");
  await page.getByLabel("Für wen / welches Programm", { exact: true }).fill("Testperson · lokaler Client");
  await page.getByLabel("Zweck / Notiz (optional)", { exact: true }).fill("Nur synthetische Prüfprojekte verwalten.");
  await page.getByLabel("Erlaubte Schnittstelle", { exact: true }).selectOption("both");
  await page.getByRole("button", { name: "Zugang und Token erzeugen", exact: true }).click();
  const secretInput = page.getByLabel("Neu erzeugtes Zugriffstoken", { exact: true });
  await expect(secretInput).toBeVisible();
  const token = await secretInput.inputValue();
  expect(/^fs_[a-f0-9]{64}$/.test(token)).toBe(true);
  await page.getByRole("button", { name: "Token kopieren", exact: true }).click();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied === token).toBe(true);
  await page.evaluate(() => navigator.clipboard.writeText(""));
  const firstList = await call("tokens.list");
  expect(firstList.length).toBe(1);
  expect(firstList[0].recipient).toBe("Testperson · lokaler Client");
  expect(firstList[0].scope).toBe("both");
  expect(Boolean(firstList[0].created_at)).toBe(true);
  expect(JSON.stringify(firstList).includes(token)).toBe(false);
  await page.getByRole("button", { name: "Token ausblenden", exact: true }).click();
  await expect(secretInput).toHaveCount(0);
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"REST, MCP & Zugänge",exact:true}).click();
  await expect(secretInput).toHaveCount(0);
  const first = page.getByRole("article", { name: "Zugang: Synthetischer Präsentationshelfer", exact: true });
  await expect(first.getByText("Testperson · lokaler Client", { exact: false })).toBeVisible();
  await first.getByRole("button",{name:"Details und Zeitangaben"}).click();
  await expect(first.getByText("Noch nicht verwendet", { exact: true })).toBeVisible();
  await first.getByRole("button", { name: "Angaben bearbeiten", exact: true }).click();
  await page.getByLabel("Für wen / welches Programm", { exact: true }).fill("Prüfteam · Client A");
  await page.getByLabel("Zweck / Notiz (optional)", { exact: true }).fill("<script>window.tokenUnsafe=true</script>");
  await page.getByRole("button", { name: "Angaben speichern", exact: true }).click();
  await expect(first.getByText("Prüfteam · Client A", { exact: false })).toBeVisible();
  expect(await page.evaluate(() => "tokenUnsafe" in window)).toBe(false);
  await page.getByRole("button", { name: "Neuen Zugang anlegen", exact: true }).click();
  await page.getByLabel("Name des Zugangs", { exact: true }).fill("Synthetischer Zweitzugang");
  await page.getByLabel("Für wen / welches Programm", { exact: true }).fill("Prüfteam · Client B");
  await page.getByRole("button", { name: "Zugang und Token erzeugen", exact: true }).click();
  await expect(secretInput).toBeVisible();
  // Closing the settings removes the one-time view, but never deletes the access grant.
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"REST, MCP & Zugänge",exact:true}).click();
  await expect(secretInput).toHaveCount(0);
  await first.getByRole("button", { name: "Zugang widerrufen", exact: true }).click();
  await page.getByRole("button",{name:/^Widerrufen \(/}).click();
  await expect(first.getByText("Widerrufen", { exact: true })).toBeVisible();
  await expect(first.getByRole("button", { name: "Zugang widerrufen", exact: true })).toHaveCount(0);
  await page.getByRole("button",{name:/^Aktiv \(/}).click();
  const second = page.getByRole("article", { name: "Zugang: Synthetischer Zweitzugang", exact: true });
  await page.getByRole("button",{name:/^Aktiv \(/}).click();
  await expect(second.getByText("Aktiv", { exact: true })).toBeVisible();
  const changed = await call("tokens.list");
  expect(changed.find((access: any) => access.id === firstList[0].id).active).toBe(false);
  expect(changed.filter((access: any) => access.active).length).toBe(1);
  expect(changed.find((access: any) => access.id === firstList[0].id).created_at).toBe(firstList[0].created_at);
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  await page.reload();
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"REST, MCP & Zugänge",exact:true}).click();
  await page.getByRole("button",{name:/^Widerrufen \(/}).click();
  await expect(first.getByText("Widerrufen", { exact: true })).toBeVisible();
  await page.getByRole("button",{name:/^Aktiv \(/}).click();
  await expect(second.getByText("Aktiv", { exact: true })).toBeVisible();
  await expect(secretInput).toHaveCount(0);
  await fs.mkdir(path.resolve(".artifacts/ui-proof"), { recursive: true });
  await page.getByRole("button",{name:/^Widerrufen \(/}).click();
  await first.scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.resolve(".artifacts/ui-proof/23-access-inventory-light.png") });
  await page.getByRole("button", { name: "Schließen", exact: true }).click();
  await call("settings.save", { theme: "dark" });
  await page.reload();
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("navigation", {name:"Einstellungsbereiche"}).getByRole("button",{name:"REST, MCP & Zugänge",exact:true}).click();
  await page.getByRole("button",{name:/^Widerrufen \(/}).click();
  await expect(first.getByText("Widerrufen", { exact: true })).toBeVisible();
  await page.getByRole("button",{name:/^Widerrufen \(/}).click();
  await first.scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.resolve(".artifacts/ui-proof/24-access-inventory-dark.png") });
  expect(await page.evaluate(() => localStorage.length)).toBe(0);
});
