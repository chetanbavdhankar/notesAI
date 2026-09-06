import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
const server = createServer(async (req, res) => {
  let body = "";
  for await (const chunk of req) body += chunk;
  if (req.url === "/v1/models") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ data: [{ id: "smoke-model" }] }));
    return;
  }
  if (req.url === "/v1/chat/completions") {
    const data = JSON.parse(body);
    if (!data.messages[0].content.includes("angular momentum")) {
      res.writeHead(400);
      res.end("Missing retrieved context");
      return;
    }
    res.writeHead(200, { "Content-Type": "text/event-stream" });
    for (const content of [
      "Planets orbit because of gravity. ",
      "Angular momentum is conserved. [^1]",
    ])
      res.write(
        `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\n`,
      );
    res.end("data: [DONE]\n\n");
    return;
  }
  res.writeHead(404);
  res.end();
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const port = server.address().port;
await mkdir(".tools/native-smoke", { recursive: true });
await mkdir("artifacts", { recursive: true });
const app = spawn(
  resolve(
    process.env.NOTESAI_SMOKE_EXE || "src-tauri/target/debug/notesai.exe",
  ),
  [],
  {
    env: {
      ...process.env,
      NOTESAI_DATA_DIR: resolve(".tools/native-smoke"),
      NOTESAI_MODEL_CACHE: resolve(".tools/test-models"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9223",
    },
    windowsHide: true,
    stdio: "pipe",
  },
);
app.stderr.on("data", (b) => process.stderr.write(b));
let browser;
try {
  for (let i = 0; i < 80; i++) {
    try {
      browser = await chromium.connectOverCDP("http://127.0.0.1:9223");
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  if (!browser)
    throw new Error("Native WebView2 did not expose the test endpoint");
  let page;
  for (let i = 0; i < 40; i++) {
    page = browser
      .contexts()
      .flatMap((c) => c.pages())
      .find((p) => p.url().includes("tauri.localhost"));
    if (page) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  if (!page) throw new Error("Native frontend did not load");
  const { expect } = await import("@playwright/test");
  await page
    .getByRole("button", { name: "New capture Ctrl N", exact: true })
    .click();
  await page
    .getByLabel("Your capture")
    .fill(
      "Native orbital mechanics\n\nPlanets orbit a star because of gravity. Orbital angular momentum is conserved.",
    );
  await page.getByRole("button", { name: "Save capture", exact: true }).click();
  await expect(page.locator(".detail-footer")).toContainText(
    "Indexed and ready",
    { timeout: 120000 },
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  if (process.env.NOTESAI_TEST_OLLAMA === "1") {
    const expected = await fetch("http://127.0.0.1:11434/api/tags").then((r) =>
      r.json(),
    );
    const names = [...new Set(expected.models.map((m) => m.name))].sort();
    await page.getByLabel("Provider", { exact: true }).selectOption("custom");
    await page.getByLabel("Provider", { exact: true }).selectOption("ollama");
    await expect(page.getByLabel("Installed Ollama models")).toBeVisible({
      timeout: 30000,
    });
    await expect(page.getByLabel("API base URL")).toHaveValue(
      "http://127.0.0.1:11434/v1",
    );
    const actual = await page
      .locator("#available-models option")
      .evaluateAll((options) => options.map((o) => o.value).filter(Boolean));
    expect(actual).toEqual(names);
    await page
      .getByLabel("Installed Ollama models")
      .selectOption(names[names.length - 1]);
    await page.getByRole("button", { name: "Appearance", exact: true }).click();
    await page.getByRole("button", { name: "Dark", exact: true }).click();
    await page.getByRole("button", { name: "Ocean palette" }).click();
    await page
      .getByRole("button", { name: "Save settings", exact: true })
      .click();
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByLabel("Model ID", { exact: true })).toHaveValue(
      names[names.length - 1],
    );
    await page.getByRole("button", { name: "Discover", exact: true }).click();
    await expect(page.getByLabel("Installed Ollama models")).toHaveValue(
      names[names.length - 1],
    );
    await page.locator(".modal").evaluate((m) => {
      m.scrollTop = 0;
    });
    await page.screenshot({ path: "artifacts/ollama-settings.png" });
    console.log(
      `PASS: Real Ollama discovery returned all ${names.length} installed models; selected model and appearance survived reload.`,
    );
  }
  await page.getByLabel("Provider", { exact: true }).selectOption("custom");
  await page.getByLabel("API base URL").fill(`http://127.0.0.1:${port}/v1`);
  await page.getByRole("button", { name: "Discover", exact: true }).click();
  await expect(page.locator("#available-models option").last()).toHaveAttribute(
    "value",
    "smoke-model",
  );
  await page
    .getByLabel("Available models", { exact: true })
    .selectOption("smoke-model");
  await page
    .getByRole("button", { name: "Save settings", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Ask your library", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Ask your library" })
    .fill("Why do planets orbit?");
  await page.getByRole("button", { name: "Send question" }).click();
  await expect(page.locator(".assistant .markdown")).toContainText(
    "Angular momentum is conserved.",
    { timeout: 30000 },
  );
  await expect(page.locator(".citation")).toHaveCount(1);
  const userBox = await page.locator(".chat-message.user").last().boundingBox();
  const assistantBox = await page
    .locator(".chat-message.assistant")
    .last()
    .boundingBox();
  expect(userBox.x).toBeGreaterThan(assistantBox.x);
  await page.locator(".citation").click();
  await expect(page.locator(".reference-drawer")).toContainText(
    "Orbital angular momentum is conserved.",
  );
  await page.screenshot({ path: "artifacts/native-chat.png" });
  if (process.env.NOTESAI_TEST_CHAT === "1") {
    await page.getByRole("button", { name: "Close reference" }).click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByLabel("Provider", { exact: true }).selectOption("ollama");
    await expect(page.getByLabel("Installed Ollama models")).toBeVisible({
      timeout: 30000,
    });
    await page
      .getByLabel("Installed Ollama models")
      .selectOption("qwen3.5:0.8b");
    await page.getByRole("button", { name: "Backup", exact: true }).click();
    await expect(
      page.getByText("One-time Google setup", { exact: true }),
    ).toBeVisible();
    await expect(page.getByLabel("Google client ID")).toBeVisible();
    await page.screenshot({ path: "artifacts/backup-settings.png" });
    await page
      .getByRole("button", { name: "Save settings", exact: true })
      .click();
    await page
      .getByRole("textbox", { name: "Ask your library" })
      .fill(
        "According to my notes, why do planets orbit a star? Answer in one sentence with a source citation.",
      );
    await page.getByRole("button", { name: "Send question" }).click();
    await expect(
      page.getByRole("button", { name: "Send question" }),
    ).toBeVisible({ timeout: 180000 });
    await expect(
      page.locator(".chat-message.assistant").last().locator(".markdown"),
    ).toContainText(/gravity/i, { timeout: 180000 });
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(
      page.locator(".chat-message.assistant").last().locator(".citation"),
    ).not.toHaveCount(0);
    await page.screenshot({ path: "artifacts/ollama-chat.png" });
    console.log(
      "PASS: Real qwen3.5:0.8b answered from indexed notes with a clickable citation and no stream error.",
    );
  }
  console.log(
    "PASS: Native capture → ONNX indexing → model discovery → hybrid RAG → streamed citation → exact source drawer.",
  );
} finally {
  await browser?.close();
  app.kill();
  server.close();
}
