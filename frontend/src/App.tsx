import { useEffect, useMemo, useRef, useState } from "react";
import init, { Engine, Draft } from "./engine/engine";
import type {
  CompatReport,
  DraftState,
  OpResult,
  PlatformSpec,
  PlatformSummary,
  PalsFootprint,
  SlotAddress,
} from "./types";
import { firstFreeCell } from "./types";
import { PlatformPicker } from "./components/PlatformPicker";
import { LoadoutCanvas } from "./components/LoadoutCanvas";
import { SlotPanel } from "./components/SlotPanel";
import { ComponentBrowser } from "./components/ComponentBrowser";
import { BomPanel } from "./components/BomPanel";

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

  const [selectedSlotAddress, setSelectedSlotAddress] = useState<string | null>(
    null
  );
  const [selectedMountId, setSelectedMountId] = useState<string | null>(null);
  const [pendingSku, setPendingSku] = useState<string | null>(null);
  const [compat, setCompat] = useState<CompatReport | null>(null);
  const [status, setStatus] = useState<Status>(null);

  const refreshState = () => {
    const draft = draftRef.current;
    if (!draft) return;
    setState(JSON.parse(draft.state()) as DraftState);
  };

  const selectPlatform = (sku: string) => {
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
    setSelectedSlotAddress(parsed.slots[0]?.id ?? null);
    setSelectedMountId(null);
    setPendingSku(null);
    setStatus({
      kind: "info",
      text: `Draft started on ${parsed.name}. Pick a slot, then a component.`,
    });
  };

  const resetDraft = () => {
    if (platformSku) selectPlatform(platformSku);
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
              pendingSku={pendingSku}
              pendingFootprint={footprintOf(compat, pendingSku)}
              onSelectMount={(id) => {
                setSelectedMountId(id);
              }}
              onCellClick={(slotAddress, col, row) => {
                if (pendingSku) equip(slotAddress, pendingSku, col, row);
              }}
              onZoneClick={(slotAddress) => {
                if (pendingSku) equip(slotAddress, pendingSku, 0, 0);
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
            onSelectSlot={(address) => {
              setSelectedSlotAddress(address);
              setPendingSku(null);
            }}
          />
          {state && (
            <ComponentBrowser
              report={compat}
              selectedSlot={selectedSlot}
              pendingSku={pendingSku}
              onPickComponent={(sku) => setPendingSku(sku)}
              onClearPending={() => setPendingSku(null)}
              onAutoPlace={(sku, cols, rows) => {
                const grid = selectedSlot?.slot.pals_grid;
                if (!selectedSlot || !grid) return;
                const cell = firstFreeCell(grid, cols, rows);
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
      />

      <footer className="app-footer">
        <p>
          Same engine as the <code>axum</code> REST API (<code>core-engine</code>{" "}
          crate), compiled to WebAssembly with the catalog seed embedded at build
          time. PALS occupancy, adapter auto-injection, and 2D anchors are all
          computed client-side.
        </p>
      </footer>
    </div>
  );
}

/** Look up the PALS footprint of a pending component from the current
 * compatibility report (entries carry footprints for auto-placement). */
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
