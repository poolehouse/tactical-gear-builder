import { useEffect, useMemo, useRef, useState } from "react";
import init, { Engine, Draft } from "./engine/engine";
import type {
  CompatReport,
  DraftState,
  KitSection,
  MaterialCoverage,
  OpResult,
  PlatformSpec,
  PlatformSummary,
  PresetSummary,
  RolePreset,
  PalsFootprint,
  SlotAddress,
} from "./types";
import { firstFreeCell, sectionOfSlotId } from "./types";
import { PlatformPicker } from "./components/PlatformPicker";
import { LoadoutCanvas } from "./components/LoadoutCanvas";
import { SlotPanel } from "./components/SlotPanel";
import { ComponentBrowser } from "./components/ComponentBrowser";
import { BomPanel } from "./components/BomPanel";
import { colorwayLabel } from "./components/CompliancePanel";

/** Typed enum values accepted by `Draft.set_compliance_context` — the same
 * snake_case strings the REST API takes (mirrors the core-engine enums). */
const COLORWAYS = [
  "multicam",
  "coyote_brown",
  "ranger_green",
  "wolf_grey",
  "black",
  "woodland_m81",
] as const;

const POLICIES = [
  "us_army_ar670_1",
  "usmc_maradmin",
  "law_enforcement_patrol",
  "law_enforcement_swat",
] as const;

const ROLES = ["breacher", "jtac", "drone_operator", "sniper_dm", "recce"] as const;

const POLICY_LABELS: Record<string, string> = {
  us_army_ar670_1: "US Army AR 670-1",
  usmc_maradmin: "USMC MARADMIN",
  law_enforcement_patrol: "LE Patrol",
  law_enforcement_swat: "LE SWAT",
};

const ROLE_LABELS: Record<string, string> = {
  breacher: "Breacher",
  jtac: "JTAC",
  drone_operator: "Drone Operator",
  sniper_dm: "Sniper / DM",
  recce: "Recce",
};

export type Status = { kind: "ok" | "error" | "info"; text: string } | null;

export function App() {
  // --- engine boot -----------------------------------------------------------
  const [engine, setEngine] = useState<Engine | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);
  const [platforms, setPlatforms] = useState<PlatformSummary[]>([]);

  useEffect(() => {
    let cancelled = false;
    init()
      .then(() => {
        if (cancelled) return;
        const engine = Engine.embedded();
        setEngine(engine);
        setPlatforms(JSON.parse(engine.platforms()) as PlatformSummary[]);
        setPresets(JSON.parse(engine.presets()) as PresetSummary[]);
      })
      .catch((error: unknown) => {
        if (!cancelled) setBootError(String(error));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // --- draft state -------------------------------------------------------------
  const draftRef = useRef<Draft | null>(null);
  const [platformSku, setPlatformSku] = useState<string | null>(null);
  const [state, setState] = useState<DraftState | null>(null);

  const [activeSection, setActiveSection] = useState<KitSection>("all");
  const [selectedSlotAddress, setSelectedSlotAddress] = useState<string | null>(
    null
  );
  const [selectedMountId, setSelectedMountId] = useState<string | null>(null);
  const [pendingSku, setPendingSku] = useState<string | null>(null);
  const [compat, setCompat] = useState<CompatReport | null>(null);
  const [status, setStatus] = useState<Status>(null);

  // --- role presets + compliance context ------------------------------------
  const [presets, setPresets] = useState<PresetSummary[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  /** "" means unset — the engine falls back to the platform default
   * colorway / no policy / no role. */
  const [ctx, setCtx] = useState<{ colorway: string; policy: string; role: string }>({
    colorway: "",
    policy: "",
    role: "",
  });

  const refreshState = () => {
    const draft = draftRef.current;
    if (!draft) return;
    setState(JSON.parse(draft.state()) as DraftState);
  };

  const startDraft = (sku: string) => {
    if (!engine) return;
    const spec = engine.platform(sku);
    if (spec === undefined) {
      setStatus({ kind: "error", text: `unknown platform sku \`${sku}\`` });
      return;
    }
    const parsed = JSON.parse(spec) as PlatformSpec;
    draftRef.current?.free();
    const draft = engine.create_draft(sku);
    draftRef.current = draft;
    setPlatformSku(sku);
    setState(JSON.parse(draft.state()) as DraftState);
    setActiveSection("all");
    setSelectedSlotAddress(parsed.slots[0]?.id ?? null);
    setSelectedMountId(null);
    setPendingSku(null);
    // A fresh draft carries no compliance context.
    setCtx({ colorway: "", policy: "", role: "" });
    setSelectedPresetId("");
    setStatus({
      kind: "info",
      text: `Draft started on ${parsed.name}. Pick a slot, then drag or click a component onto 1\u00d71 MOLLE cuts.`,
    });
  };

  /** Re-clicking the active platform must not wipe the build — Reset draft
   * is the explicit way to start over. */
  const selectPlatform = (sku: string) => {
    if (sku === platformSku && draftRef.current) {
      setStatus({
        kind: "info",
        text: `Already building on ${state?.platform.name ?? sku}. Use “Reset draft” to start over.`,
      });
      return;
    }
    startDraft(sku);
  };

  const resetDraft = () => {
    if (platformSku) startDraft(platformSku);
  };

  // --- slot list (platform slots + child slots of mounted components) ----------
  const slots: SlotAddress[] = useMemo(() => {
    if (!state) return [];
    const platformSlots: SlotAddress[] = state.platform.slots.map((slot) => ({
      address: slot.id,
      slot,
      host: { kind: "platform" },
    }));
    const childSlots: SlotAddress[] = state.mounted
      .filter((item) => !item.is_adapter)
      .flatMap((item) =>
        item.child_slots.map((slot) => ({
          address: `${item.mount_id}.${slot.id}`,
          slot,
          host: { kind: "mounted", mount_id: item.mount_id },
        }))
      );
    return [...platformSlots, ...childSlots];
  }, [state]);

  const selectedSlot = useMemo(
    () => slots.find((slot) => slot.address === selectedSlotAddress) ?? null,
    [slots, selectedSlotAddress]
  );

  // --- compatibility report for the selected slot ------------------------------
  useEffect(() => {
    if (!engine || !selectedSlot) {
      setCompat(null);
      return;
    }
    const report = engine.compatible_on_slot(JSON.stringify(selectedSlot.slot));
    setCompat(report === undefined ? null : (JSON.parse(report) as CompatReport));
  }, [engine, selectedSlot]);

  // --- draft operations ---------------------------------------------------------
  const equip = (slotAddress: string, sku: string, col: number, row: number) => {
    const draft = draftRef.current;
    if (!draft) return;
    const result = JSON.parse(draft.equip(slotAddress, sku, col, row)) as OpResult;
    refreshState();
    if (result.ok) {
      // One placement completes the pending action.
      setPendingSku(null);
      setStatus({ kind: "ok", text: `Equipped \`${sku}\` on \`${slotAddress}\`.` });
    } else {
      setStatus({ kind: "error", text: result.error ?? "unknown failure" });
    }
  };

  const unequip = (mountId: string) => {
    const draft = draftRef.current;
    if (!draft) return;
    const result = JSON.parse(draft.unequip(mountId)) as OpResult;
    refreshState();
    if (result.ok) {
      setStatus({ kind: "ok", text: `Unequipped ${mountId}.` });
    } else {
      setStatus({ kind: "error", text: result.error ?? "unknown failure" });
    }
    // Clear any selection anchored to the removed subtree.
    if (selectedMountId === mountId) setSelectedMountId(null);
    if (selectedSlotAddress?.startsWith(`${mountId}.`)) {
      setSelectedSlotAddress(null);
    }
  };

  // --- compliance context & role presets -----------------------------------

  const applyComplianceContext = (
    colorway: string,
    policy: string,
    role: string
  ) => {
    const draft = draftRef.current;
    if (!draft) return;
    setCtx({ colorway, policy, role });
    const result = JSON.parse(
      draft.set_compliance_context(
        colorway || undefined,
        policy || undefined,
        role || undefined
      )
    ) as OpResult;
    refreshState();
    if (!result.ok) {
      setStatus({ kind: "error", text: result.error ?? "unknown failure" });
    }
  };

  const loadPreset = (presetId: string) => {
    if (!engine || !presetId) return;
    const raw = engine.preset(presetId);
    if (raw === undefined) {
      setStatus({ kind: "error", text: `unknown preset id \`${presetId}\`` });
      return;
    }
    const preset = JSON.parse(raw) as RolePreset;
    let draft: Draft;
    try {
      draft = engine.load_preset(presetId);
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
      return;
    }
    draftRef.current?.free();
    draftRef.current = draft;
    const parsed = JSON.parse(draft.state()) as DraftState;
    setState(parsed);
    setPlatformSku(parsed.platform.sku);
    setSelectedPresetId(presetId);
    setCtx({
      colorway: preset.default_colorway,
      policy: preset.policy,
      role: preset.role ?? "",
    });
    setActiveSection("all");
    setSelectedSlotAddress(parsed.platform.slots[0]?.id ?? null);
    setSelectedMountId(null);
    setPendingSku(null);
    setStatus({
      kind: "ok",
      text: `Loaded ${preset.name} — ${preset.placements.length} placements, validated under ${POLICY_LABELS[preset.policy] ?? preset.policy}.`,
    });
  };

  const equipSuggested = (sku: string) => {
    if (!engine || !state) return;
    for (const slot of slots) {
      const report = engine.compatible_on_slot(JSON.stringify(slot.slot));
      if (report === undefined) continue;
      const parsed = JSON.parse(report) as CompatReport;
      const entry = [...parsed.native_fit, ...parsed.requires_adapter].find(
        (candidate) => candidate.sku === sku
      );
      if (!entry) continue;
      const grid = slot.slot.pals_grid;
      if (grid && entry.pals_footprint) {
        const cell = firstFreeCell(
          grid,
          entry.pals_footprint.cols,
          entry.pals_footprint.rows,
          entry.material_coverage
        );
        if (!cell) continue;
        equip(slot.address, sku, cell[0], cell[1]);
        return;
      }
      equip(slot.address, sku, 0, 0);
      return;
    }
    setPendingSku(sku);
    setStatus({
      kind: "info",
      text: `\`${sku}\` does not fit any current slot — select a compatible slot, then place it from the component browser.`,
    });
  };

  const handleSelectSection = (section: KitSection) => {
    setActiveSection(section);
    if (section === "all" || !state) return;
    // Keep the current slot if it already belongs to this section; otherwise
    // pick the first slot in that section so the browser stays ready.
    const matches = (addr: string) => {
      const sec = sectionOfSlotId(addr, state.mounted);
      return section === "sub_load" ? sec === "sub_load" || sec === "belt" : sec === section;
    };
    if (selectedSlotAddress && matches(selectedSlotAddress)) return;
    const firstInSec = slots.find((s) => matches(s.address));
    if (firstInSec) {
      setSelectedSlotAddress(firstInSec.address);
    }
  };

  const selectedMount = useMemo(
    () => state?.mounted.find((item) => item.mount_id === selectedMountId) ?? null,
    [state, selectedMountId]
  );

  // --- render --------------------------------------------------------------------
  if (bootError) {
    return (
      <div className="boot-error">
        <h1>Engine failed to load</h1>
        <p>{bootError}</p>
      </div>
    );
  }

  if (!engine) {
    return (
      <div className="boot-loading">
        <span className="led" aria-hidden="true" /> compiling engine&hellip;
      </div>
    );
  }

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-title">
          <span className="prompt">$</span> tactical-gear-builder
          <span className="app-sub">
            // rust compatibility core &middot; wasm &middot; zero network calls
          </span>
        </div>
        <div className="app-actions">
          {platformSku && (
            <>
              <span className="catalog-count">
                {platforms.length} platforms &middot; runs in your browser
              </span>
              <button className="btn btn-ghost" onClick={resetDraft}>
                Reset draft
              </button>
            </>
          )}
        </div>
      </header>

      <div className="context-bar">
        <label className="ctx-group">
          <span className="ctx-label">role preset</span>
          <select
            className="ctx-select"
            value={selectedPresetId}
            disabled={!engine || presets.length === 0}
            onChange={(event) => setSelectedPresetId(event.target.value)}
          >
            <option value="">&mdash; manual build &mdash;</option>
            {presets.map((preset) => (
              <option key={preset.preset_id} value={preset.preset_id}>
                {preset.name} ({preset.placement_count} placements)
              </option>
            ))}
          </select>
        </label>
        <button
          className="btn"
          disabled={!engine || !selectedPresetId}
          onClick={() => loadPreset(selectedPresetId)}
        >
          Load preset
        </button>
        <label className="ctx-group">
          <span className="ctx-label">colorway</span>
          <select
            className="ctx-select"
            value={ctx.colorway}
            disabled={!platformSku}
            onChange={(event) =>
              applyComplianceContext(event.target.value, ctx.policy, ctx.role)
            }
          >
            <option value="">platform default</option>
            {COLORWAYS.map((colorway) => (
              <option key={colorway} value={colorway}>
                {colorwayLabel(colorway)}
              </option>
            ))}
          </select>
        </label>
        <label className="ctx-group">
          <span className="ctx-label">regulation</span>
          <select
            className="ctx-select"
            value={ctx.policy}
            disabled={!platformSku}
            onChange={(event) =>
              applyComplianceContext(ctx.colorway, event.target.value, ctx.role)
            }
          >
            <option value="">none</option>
            {POLICIES.map((policy) => (
              <option key={policy} value={policy}>
                {POLICY_LABELS[policy] ?? policy}
              </option>
            ))}
          </select>
        </label>
        <label className="ctx-group">
          <span className="ctx-label">role</span>
          <select
            className="ctx-select"
            value={ctx.role}
            disabled={!platformSku}
            onChange={(event) =>
              applyComplianceContext(ctx.colorway, ctx.policy, event.target.value)
            }
          >
            <option value="">none</option>
            {ROLES.map((role) => (
              <option key={role} value={role}>
                {ROLE_LABELS[role] ?? role}
              </option>
            ))}
          </select>
        </label>
      </div>

      <main className="app-grid">
        <PlatformPicker
          platforms={platforms}
          platformSku={platformSku}
          onSelect={selectPlatform}
        />

        <section className="canvas-pane">
          {state ? (
            <LoadoutCanvas
              state={state}
              slots={slots}
              selectedSlot={selectedSlot}
              selectedMountId={selectedMountId}
              activeSection={activeSection}
              pendingSku={pendingSku}
              pendingFootprint={footprintOf(compat, pendingSku)}
              pendingCoverage={coverageOf(compat, pendingSku)}
              onSelectSection={handleSelectSection}
              onSelectMount={(id) => {
                setSelectedMountId(id);
              }}
              onCellClick={(slotAddress, col, row) => {
                if (pendingSku) equip(slotAddress, pendingSku, col, row);
              }}
              onZoneClick={(slotAddress) => {
                if (pendingSku) equip(slotAddress, pendingSku, 0, 0);
              }}
              onDropOnCell={(slotAddress, sku, col, row) => {
                equip(slotAddress, sku, col, row);
              }}
              onDropOnZone={(slotAddress, sku) => {
                equip(slotAddress, sku, 0, 0);
              }}
            />
          ) : (
            <div className="canvas-empty">
              Select a platform to start a loadout.
            </div>
          )}
          <StatusLine status={status} onDismiss={() => setStatus(null)} />
        </section>

        <section className="right-pane">
          <SlotPanel
            slots={slots}
            selectedSlotAddress={selectedSlotAddress}
            state={state}
            selectedMountId={selectedMountId}
            activeSection={activeSection}
            onSelectSlot={(address) => {
              setSelectedSlotAddress(address);
              setPendingSku(null);
            }}
            onSelectMount={(id) => setSelectedMountId(id)}
          />
          {state && (
            <ComponentBrowser
              report={compat}
              selectedSlot={selectedSlot}
              pendingSku={pendingSku}
              onPickComponent={(sku) => setPendingSku(sku)}
              onClearPending={() => setPendingSku(null)}
              onAutoPlace={(sku, cols, rows, coverage) => {
                const grid = selectedSlot?.slot.pals_grid;
                if (!selectedSlot || !grid) return;
                const cell = firstFreeCell(grid, cols, rows, coverage);
                if (cell) {
                  equip(selectedSlot.address, sku, cell[0], cell[1]);
                } else {
                  setStatus({
                    kind: "error",
                    text: `No free ${cols}\u00d7${rows} cell block on \`${selectedSlot.address}\`.`,
                  });
                }
              }}
            />
          )}
        </section>
      </main>

      <BomPanel
        state={state}
        selectedMount={selectedMount}
        onUnequip={unequip}
        onSuggestedSku={equipSuggested}
      />

      <footer className="app-footer">
        <p>
          Same engine as the <code>axum</code> REST API (<code>core-engine</code>{" "}
          crate), compiled to WebAssembly with the catalog seed embedded at build
          time. PALS occupancy, material coverage overhang, adapter auto-injection,
          and 2D/3D section anchors are all computed client-side.
        </p>
      </footer>
    </div>
  );
}

/** Look up the PALS attachment footprint of a pending component from the current
 * compatibility report. */
function footprintOf(
  report: CompatReport | null,
  sku: string | null
): PalsFootprint | null {
  if (!report || !sku) return null;
  for (const entry of [
    ...report.native_fit,
    ...report.requires_adapter,
    ...report.incompatible,
  ]) {
    if (entry.sku === sku) return entry.pals_footprint ?? null;
  }
  return null;
}

/** Look up the Material Coverage footprint of a pending component from the
 * current compatibility report. */
function coverageOf(
  report: CompatReport | null,
  sku: string | null
): MaterialCoverage | null {
  if (!report || !sku) return null;
  for (const entry of [
    ...report.native_fit,
    ...report.requires_adapter,
    ...report.incompatible,
  ]) {
    if (entry.sku === sku) return entry.material_coverage ?? null;
  }
  return null;
}

function StatusLine({
  status,
  onDismiss,
}: {
  status: Status;
  onDismiss: () => void;
}) {
  if (!status) return <div className="status status-empty">&nbsp;</div>;
  return (
    <div className={`status status-${status.kind}`} role="status">
      <span className="status-text">{status.text}</span>
      <button className="status-dismiss" onClick={onDismiss} aria-label="Dismiss">
        &times;
      </button>
    </div>
  );
}
