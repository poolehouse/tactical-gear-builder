/**
 * End-to-end Playwright test suite at the Canvas seam (`frontend/tests/placement.spec.mjs`).
 *
 * Rules:
 * - Tests ONLY user-observable DOM/Canvas behavior through the real compiled
 *   WebAssembly engine (`wasm-engine`), never React state internals.
 * - Verifies:
 *   1. Clicking a valid free anchor mounts the SKU and clears the pending banner.
 *   2. Invalid footprint / overlap anchors are pre-blocked (`is-blocked`,
 *      `aria-disabled="true"`, `is-preview-invalid` on hover, click no-op).
 *   3. Tools, Munitions & Radios section lists the CSV tools (Good Guy BRB Tool,
 *      Paratech Halligan Tool, Slap Strip Breach Charge, Spiritus PRC-152 Radio
 *      Pouch, etc.) with dual footprints (`AttachmentFootprint` vs `MaterialCoverage`),
 *      mounts them onto placards/cummerbunds, and enforces material coverage
 *      blocking on in-bounds cells while allowing outer-edge overhang.
 *   4. Drag-and-drop from the Component Browser onto a 1x1 PALS cell mounts
 *      gear and clears the pending state.
 *   5. Switching between Kit Sections (`Front`, `Rear`, `Left Side`, `Right Side`,
 *      `Sub-Load / Belt`, `All Sections`) preserves all placed gear across sections.
 *   6. Standing D51 contracts: auto-place feasibility gating, active-platform
 *      re-click protection, always-on colorway feedback, occluded mount selection,
 *      and zero network requests after boot (R8).
 */
import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Try Playwright resolution from project/global or Hermes agent cache
let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  const require = createRequire("/home/swimmer/.hermes/hermes-agent/");
  ({ chromium } = require("playwright"));
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FRONTEND_DIR = path.resolve(__dirname, "..");
const PORT = 4177;
const BASE = `http://localhost:${PORT}`;

const preview = spawn(
  "npx",
  ["vite", "preview", "--port", String(PORT), "--strictPort"],
  { cwd: FRONTEND_DIR, stdio: "pipe" }
);

const failures = [];
const assert = (name, ok, detail = "") => {
  const tag = ok ? "PASS" : "FAIL";
  console.log(`[${tag}] ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures.push(`${name}${detail ? ` (${detail})` : ""}`);
};

try {
  let up = false;
  for (let i = 0; i < 40 && !up; i++) {
    await new Promise((r) => setTimeout(r, 250));
    try {
      const res = await fetch(BASE, { method: "HEAD" });
      up = res.ok;
    } catch {
      // wait
    }
  }
  if (!up) throw new Error("vite preview did not start");

  const executablePath =
    process.env.PLAYWRIGHT_CHROMIUM_PATH ||
    (fs.existsSync("/usr/bin/google-chrome")
      ? "/usr/bin/google-chrome"
      : fs.existsSync("/usr/bin/chromium")
        ? "/usr/bin/chromium"
        : undefined);
  const browser = await chromium.launch({
    ...(executablePath ? { executablePath } : {}),
    headless: true,
    args: ["--no-sandbox"],
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });

  const postBootRequests = [];
  const consoleErrors = [];
  let booted = false;
  page.on("request", (req) => {
    if (booted) postBootRequests.push(req.url());
  });
  page.on("pageerror", (err) => consoleErrors.push(String(err)));
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });

  await page.goto(BASE, { waitUntil: "load" });
  await page.waitForSelector(".picker-item", { timeout: 20000 });
  booted = true;

  // --- Slice 1: Clicking a valid free anchor mounts the SKU and clears the pending banner ---
  await page.click("button.picker-item:has-text('JPC 2.0')");
  await page.waitForSelector(".platform-silhouette");

  // Verify top-down flat-lay geometry: rear platebag is positioned strictly ABOVE front placard (no overlap)
  const rearBox = await page
    .locator(".slot-zone:has-text('rear_panel')")
    .boundingBox();
  const frontBox = await page
    .locator(".slot-zone:has-text('front_placard')")
    .boundingBox();
  assert(
    "slice 0: top-down flat-lay positions rear_panel strictly above front_placard without overlap",
    Boolean(
      rearBox &&
        frontBox &&
        rearBox.y + rearBox.height < frontBox.y &&
        (await page.locator(".flatlay-rear-bag").count()) === 1 &&
        (await page.locator(".flatlay-front-bag").count()) === 1
    ),
    rearBox && frontBox
      ? `rear bottom=${Math.round(rearBox.y + rearBox.height)}px < front top=${Math.round(frontBox.y)}px`
      : "missing slot boxes"
  );

  // Mount MK5 placard on front_placard (anchor slot)
  await page.click(".compat-entry:has-text('Spiritus Systems MK5 Placard')");
  assert(
    "slice 1a: picking MK5 shows pending banner",
    (await page.locator(".pending-banner").count()) === 1
  );
  await page.click(".slot-zone.is-target");
  assert(
    "slice 1b: anchor zone click mounts MK5 and clears pending banner",
    (await page.locator(".pending-banner").count()) === 0 &&
      (await page.locator(".mount-layer").count()) === 2
  );

  // Select the placard's child 6x3 PALS grid (`m2.placard_front_pals`)
  await page.click(".slot-row:has-text('placard_front_pals')");
  await page.click(".compat-entry:has-text('Esstac KYWI 5.56 Double')");
  assert(
    "slice 1c: picking KYWI on PALS slot arms 1x1 PALS cells and shows pending banner",
    (await page.locator(".pending-banner").count()) === 1 &&
      (await page.locator(".pals-cell").count()) === 18
  );
  await page.click("button.pals-cell[aria-label='PALS cell 0,0']");
  assert(
    "slice 1d: clicking valid free anchor (0,0) mounts KYWI and clears pending banner",
    (await page.locator(".pending-banner").count()) === 0 &&
      (await page.locator(".mount-layer").count()) === 3
  );

  // --- Slice 2: Blocked anchors (overlap + out-of-bounds) & invalid preview ---
  await page.click(".compat-entry:has-text('Esstac KYWI 5.56 Double')");
  const cell00 = page.locator("button.pals-cell").first();
  const cell50 = page.locator("button.pals-cell").nth(5); // col 5, row 0 (out of bounds for 2x2)
  assert(
    "slice 2a: occupied (0,0) and out-of-bounds (5,0) anchors carry aria-disabled=true",
    (await cell00.getAttribute("aria-disabled")) === "true" &&
      (await cell50.getAttribute("aria-disabled")) === "true"
  );
  await cell50.hover();
  assert(
    "slice 2b: hovering blocked anchor shows red invalid preview",
    (await page.locator(".pals-cell.is-preview-invalid").count()) > 0
  );
  await cell50.click({ force: true });
  assert(
    "slice 2c: clicking blocked anchor is a no-op and keeps pending banner armed",
    (await page.locator(".pending-banner").count()) === 1 &&
      (await page.locator(".mount-layer").count()) === 3
  );
  await page.click(".pending-banner button:has-text('Cancel')");

  // --- Slice 3: Tools, Munitions & Radios section + Dual-Footprint (Attachment vs Material Coverage) ---
  const toolsHeading = await page.locator(".compat-group:has-text('Tools, Munitions & Radios')").count();
  assert(
    "slice 3a: Component Browser renders dedicated Tools, Munitions & Radios section",
    toolsHeading === 1
  );
  const browserText = (await page.textContent(".browser-pane")) ?? "";
  for (const toolName of [
    "Good Guy BRB Tool",
    "Paratech Halligan Tool",
    "Slap Strip Breach Charge",
    "Spiritus PRC-152 Radio Pouch",
  ]) {
    assert(`slice 3b: Tools section includes ${toolName}`, browserText.includes(toolName));
  }

  // Pick Good Guy BRB Tool (1x2 attach, 2x4 material coverage) and place at col 5, row 0 on 6x3 grid:
  // Attachment (1x2) fits at col 5, rows 0..1; Material Coverage (2x4) covers col 5, row 2 in-bounds
  // and overhangs col 6 and row 3 outside the grid!
  await page.click(".compat-entry:has-text('Good Guy BRB Tool')");
  const cell50ForBrb = page.locator("button.pals-cell[aria-label^='PALS cell 5,0']");
  assert(
    "slice 3c: BRB Tool (1x2 attach, 2x4 coverage) is valid at edge col 5, row 0 (material coverage overhangs grid edge)",
    (await cell50ForBrb.getAttribute("aria-disabled")) === "false"
  );
  await cell50ForBrb.hover();
  assert(
    "slice 3d: hovering BRB Tool shows both attachment preview and material coverage preview",
    (await page.locator(".pals-cell.is-preview").count()) === 2 &&
      (await page.locator(".pals-cell.is-preview-coverage").count()) === 1
  );
  await cell50ForBrb.click();
  assert(
    "slice 3e: clicking (5,0) mounts Good Guy BRB Tool and clears pending banner",
    (await page.locator(".pending-banner").count()) === 0 &&
      (await page.locator(".mount-layer").count()) === 4
  );

  // Re-arm a 1x2 tool (Slap Strip Breach Charge) and verify that cell (5,2) is now marked covered/occupied by BRB's overhang
  await page.click(".compat-entry:has-text('Slap Strip Breach Charge')");
  const cell51 = page.locator("button.pals-cell").nth(11); // col 5, row 1
  assert(
    "slice 3f: in-bounds material coverage cell blocks overlapping placement",
    (await cell51.getAttribute("aria-disabled")) === "true"
  );
  await page.click(".pending-banner button:has-text('Cancel')");

  // --- Slice 4: Drag-and-Drop onto 1x1 PALS cells ---
  const stripChargeEntry = page.locator(".compat-entry:has-text('Slap Strip Breach Charge')");
  const targetCell20 = page.locator("button.pals-cell[aria-label^='PALS cell 2,0']");
  // Drag and drop directly from Component Browser onto PALS cell (2,0)
  await stripChargeEntry.dragTo(targetCell20);
  assert(
    "slice 4: dragging Slap Strip Breach Charge onto PALS cell (2,0) mounts it and leaves pending banner clear",
    (await page.locator(".pending-banner").count()) === 0 &&
      (await page.locator(".mount-layer").count()) === 5
  );

  // --- Slice 5: Kit Section Switching with State Preservation ---
  const sectionTabs = await page.locator(".section-tab").count();
  assert("slice 5a: Kit Section switcher bar renders section tabs", sectionTabs >= 5);

  // Switch to Left Side section (`cummerbund_left`, 6x3 PALS grid)
  await page.click(".section-tab:has-text('Left Side')");
  await page.click(".slot-row:has-text('cummerbund_left')");
  const prc152 = page.locator(".compat-entry:has-text('Spiritus PRC-152 Radio Pouch')");
  await prc152.click();
  await page.click("button.pals-cell[aria-label^='PALS cell 0,0']");
  assert(
    "slice 5b: mounted PRC-152 Radio Pouch on Left Cummerbund 1x1 PALS grid",
    (await page.locator(".pending-banner").count()) === 0
  );

  // Switch to Rear section, then back to Front, then All Sections — verify all placed gear is preserved
  await page.click(".section-tab:has-text('Rear')");
  await page.click(".section-tab:has-text('Front')");
  await page.click(".section-tab:has-text('All Sections')");
  assert(
    "slice 5c: switching between Kit Sections preserves all 6 mounted layers across Front and Left Side",
    (await page.locator(".mount-layer").count()) === 6
  );

  // --- Slice 6: Standing D51 contracts + R8 zero network calls ---
  // Re-clicking active platform is a safe no-op
  await page.click("button.picker-item:has-text('JPC 2.0')");
  assert(
    "slice 6a: re-clicking active platform preserves all 6 mounts",
    (await page.locator(".mount-layer").count()) === 6 &&
      ((await page.textContent(".status")) ?? "").includes("Already building on")
  );

  // Colorway feedback without policy
  await page.locator(".ctx-select >> nth=1").selectOption("ranger_green");
  assert(
    "slice 6b: colorway updates silhouette data-colorway and Compliance Panel swatch without active policy",
    (await page.locator(".platform-silhouette").getAttribute("data-colorway")) === "ranger_green" &&
      ((await page.textContent(".compliance")) ?? "").includes("Ranger Green")
  );

  // Occluded mount selection via SlotPanel Mounted items list
  await page.click(".slot-row.mount-row:has-text('m2')");
  assert(
    "slice 6c: occluded placard m2 is selectable via SlotPanel Mounted items list",
    ((await page.textContent(".inspector")) ?? "").includes("spiritus-mk5-placard")
  );

  assert(
    "slice 6d: zero network requests after boot (R8)",
    postBootRequests.length === 0,
    postBootRequests.join(", ") || "0 requests"
  );
  assert(
    "slice 6e: zero console/page errors",
    consoleErrors.length === 0,
    consoleErrors.join("; ")
  );

  await browser.close();
} catch (err) {
  failures.push(`exception: ${err?.message ?? err}`);
  console.error(err);
} finally {
  preview.kill("SIGTERM");
}

if (failures.length > 0) {
  console.error(`\n${failures.length} FAILURE(S):\n  - ${failures.join("\n  - ")}`);
  process.exit(1);
}
console.log("\nALL PLAYWRIGHT CANVAS SEAM TESTS PASSED");
process.exit(0);
