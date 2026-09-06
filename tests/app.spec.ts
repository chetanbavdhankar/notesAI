import { expect, test } from "@playwright/test";
test("capture, edit, filter, reload, and delete persist correctly", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Your knowledge, within reach." }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "New capture Ctrl N", exact: true })
    .click();
  await page
    .getByLabel("Your capture")
    .fill(
      "Orbital mechanics\n\nUse conservation of angular momentum to understand planetary motion.",
    );
  await page.getByLabel("Tags").fill("science, physics");
  await page.getByRole("button", { name: "Save capture" }).click();
  await expect(
    page
      .getByRole("heading", { name: "Orbital mechanics", exact: true })
      .last(),
  ).toBeVisible();
  await page.getByRole("button", { name: "Edit note", exact: true }).click();
  await page.getByLabel("Title", { exact: true }).fill("Planetary motion");
  await page
    .getByRole("textbox", { name: "Markdown", exact: true })
    .fill("Angular momentum stays constant in an isolated system.");
  await page.getByRole("button", { name: "Save changes" }).click();
  await page.reload();
  await page
    .getByRole("textbox", { name: "Search captures" })
    .fill("Planetary");
  await expect(page.locator(".note-card")).toHaveCount(1);
  await page.locator(".note-card").click();
  await expect(page.locator(".note-detail")).toContainText(
    "Angular momentum stays constant",
  );
  await page.screenshot({ path: "artifacts/library.png", fullPage: true });
  await page.getByRole("button", { name: "Delete note", exact: true }).click();
  await page
    .getByRole("button", { name: "Delete capture", exact: true })
    .click();
  await expect(page.locator(".note-card")).toHaveCount(0);
  expect(errors).toEqual([]);
});
test("appearance previews, cancels, persists, and follows system changes", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  const sageLogoFill = await page
    .locator(".brand-tile")
    .evaluate((element) => getComputedStyle(element).fill);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  await page.getByRole("button", { name: "Dark", exact: true }).click();
  await page.getByRole("button", { name: "Ocean palette" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("html")).toHaveAttribute("data-palette", "blue");
  const oceanLogoFill = await page
    .locator(".brand-tile")
    .evaluate((element) => getComputedStyle(element).fill);
  expect(oceanLogoFill).not.toBe(sageLogoFill);
  await page.screenshot({
    path: "artifacts/settings-dark-ocean.png",
    animations: "disabled",
  });
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page.locator("html")).toHaveAttribute("data-palette", "sage");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  await page
    .getByRole("button", { name: "Follow system", exact: true })
    .click();
  await page.getByLabel("Custom accent color").fill("#df528a");
  await page
    .getByRole("button", { name: "Save settings", exact: true })
    .click();
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-palette", "custom");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  await expect(page.getByLabel("Custom accent color")).toHaveValue("#df528a");
  await page.getByRole("button", { name: "Violet palette" }).click();
  await page
    .getByRole("button", { name: "Save settings", exact: true })
    .click();
  await page.screenshot({ path: "artifacts/library-dark-violet.png" });
});
test("model profiles persist and preview chat reports its real boundary", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByLabel("Profile name").fill("My local model");
  await page.getByRole("button", { name: "Retrieval", exact: true }).click();
  await page.getByLabel("Top K passages").fill("9");
  await page.getByRole("button", { name: "Save settings" }).click();
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByLabel("Profile name")).toHaveValue("My local model");
  await page.getByRole("button", { name: "Retrieval", exact: true }).click();
  await expect(page.getByLabel("Top K passages")).toHaveValue("9");
  await page.getByRole("button", { name: "Close dialog" }).click();
  await page
    .getByRole("button", { name: "Ask your library", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Ask your library" })
    .fill("What did I save?");
  await page.getByRole("button", { name: "Send question" }).click();
  await expect(page.getByRole("alert")).toContainText("Run the desktop app");
});

test("settings sections isolate controls and explain Google setup", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByLabel("Profile name")).toBeVisible();
  await expect(page.getByLabel("Google client ID")).toHaveCount(0);
  await page.getByRole("button", { name: "Backup", exact: true }).click();
  await expect(page.getByLabel("Profile name")).toHaveCount(0);
  await expect(page.getByLabel("Google client ID")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Sign in with Google" }),
  ).toBeDisabled();
  await page.getByLabel("Check every (minutes)").fill("30");
  await page
    .getByRole("button", { name: "Save settings", exact: true })
    .click();
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Backup", exact: true }).click();
  await expect(page.getByLabel("Check every (minutes)")).toHaveValue("30");
});
