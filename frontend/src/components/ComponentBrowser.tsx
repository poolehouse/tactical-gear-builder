import type {
  CompatReport,
  MaterialCoverage,
  PalsMatrix,
  SlotAddress,
} from "../types";
import { firstFreeCell, isToolOrMunitionSku, prettifyEnum } from "../types";

/** `IncompatibilityReason` serializes as a single-key object
 * (`{"InterfaceFamilyMismatch": {...}}`); extract the variant name. */
function reasonLabel(reason: unknown): string {
  if (reason && typeof reason === "object") {
    const key = Object.keys(reason as object)[0];
    if (key) return prettifyEnum(key);
  }
  return "";
}

/**
 * Compatibility browser for the selected slot: every catalog component,
 * classified by the engine into native fits, Tools/Munitions/Radios,
 * adapter-bridged mounts, and hard incompatibilities.
 */
export function ComponentBrowser({
  report,
  selectedSlot,
  pendingSku,
  onPickComponent,
  onClearPending,
  onAutoPlace,
}: {
  report: CompatReport | null;
  selectedSlot: SlotAddress | null;
  pendingSku: string | null;
  onPickComponent: (sku: string) => void;
  onClearPending: () => void;
  onAutoPlace: (
    sku: string,
    cols: number,
    rows: number,
    coverage?: MaterialCoverage | null
  ) => void;
}) {
  if (!selectedSlot) {
    return (
      <section className="pane browser-pane">
        <h2 className="pane-title">Components</h2>
        <p className="slot-hint">Select a mount slot first.</p>
      </section>
    );
  }

  if (!report) {
    return (
      <section className="pane browser-pane">
        <h2 className="pane-title">Components</h2>
        <p className="slot-hint">Classifying&hellip;</p>
      </section>
    );
  }

  const grid = selectedSlot.slot.pals_grid ?? null;
  const hasGrid = grid !== null;

  const coreNative = report.native_fit.filter((e) => !isToolOrMunitionSku(e.sku));
  const toolsNative = report.native_fit.filter((e) => isToolOrMunitionSku(e.sku));
  const toolsIncompatible = report.incompatible.filter((e) =>
    isToolOrMunitionSku(e.sku)
  );
  const coreIncompatible = report.incompatible.filter(
    (e) => !isToolOrMunitionSku(e.sku)
  );

  return (
    <section className="pane browser-pane">
      <h2 className="pane-title">
        Components <span className="pane-count">// for {selectedSlot.address}</span>
      </h2>

      {pendingSku && (
        <div className="pending-banner">
          <span>
            Placing <code>{pendingSku}</code> —{" "}
            {hasGrid
              ? "click or drop onto a 1\u00d71 cell on the highlighted slot."
              : "click or drop onto the highlighted slot."}
          </span>
          <button className="btn btn-ghost" onClick={onClearPending}>
            Cancel
          </button>
        </div>
      )}

      <CompatGroup
        title="Native fit"
        note="mounts directly · drag or click"
        entries={coreNative}
        variant="native"
        pendingSku={pendingSku}
        grid={grid}
        onPickComponent={onPickComponent}
        onAutoPlace={onAutoPlace}
      />
      <CompatGroup
        title="Tools, Munitions & Radios"
        note={
          toolsNative.length > 0
            ? "affix to placard / cummerbund / panel PALS"
            : "requires a 1\u00d71 PALS grid slot (mount a placard or select a PALS field)"
        }
        entries={toolsNative.length > 0 ? toolsNative : toolsIncompatible}
        variant={toolsNative.length > 0 ? "tools" : "incompatible"}
        pendingSku={pendingSku}
        grid={grid}
        onPickComponent={onPickComponent}
        onAutoPlace={onAutoPlace}
      />
      <CompatGroup
        title="Requires adapter"
        note="auto-injected into the BOM"
        entries={report.requires_adapter}
        variant="adapter"
        pendingSku={pendingSku}
        grid={grid}
        onPickComponent={onPickComponent}
        onAutoPlace={onAutoPlace}
      />
      <CompatGroup
        title="Incompatible"
        note="no single-hop mount exists"
        entries={coreIncompatible}
        variant="incompatible"
        pendingSku={pendingSku}
        grid={grid}
        onPickComponent={onPickComponent}
        onAutoPlace={onAutoPlace}
      />
    </section>
  );
}

function CompatGroup({
  title,
  note,
  entries,
  variant,
  pendingSku,
  grid,
  onPickComponent,
  onAutoPlace,
}: {
  title: string;
  note: string;
  entries: CompatReport["native_fit"];
  variant: "native" | "tools" | "adapter" | "incompatible";
  pendingSku: string | null;
  grid: PalsMatrix | null;
  onPickComponent: (sku: string) => void;
  onAutoPlace: (
    sku: string,
    cols: number,
    rows: number,
    coverage?: MaterialCoverage | null
  ) => void;
}) {
  if (entries.length === 0) return null;
  const isDisabled = variant === "incompatible";
  return (
    <div className={`compat-group compat-${variant}`}>
      <h3 className="compat-title">
        {title} <span className="compat-note">// {note}</span>
      </h3>
      <ul className="compat-list">
        {entries.map((entry) => {
          const footprint = entry.pals_footprint;
          const coverage = entry.material_coverage;
          const hasOverhang = Boolean(
            footprint &&
              coverage &&
              (coverage.cols !== footprint.cols || coverage.rows !== footprint.rows)
          );
          const canAutoPlace = Boolean(
            footprint &&
              grid &&
              firstFreeCell(grid, footprint.cols, footprint.rows, coverage) !== null
          );
          return (
            <li key={entry.sku}>
              <button
                className={`compat-entry${pendingSku === entry.sku ? " is-pending" : ""}`}
                onClick={() => onPickComponent(entry.sku)}
                disabled={isDisabled}
                draggable={!isDisabled}
                onDragStart={(event) => {
                  if (isDisabled) return;
                  event.dataTransfer.setData("text/plain", entry.sku);
                  event.dataTransfer.effectAllowed = "copy";
                  onPickComponent(entry.sku);
                }}
              >
                <span className="compat-name">{entry.name}</span>
                <span className="compat-kind">{prettifyEnum(entry.kind)}</span>
                {footprint && (
                  <span className="compat-footprint">
                    {footprint.cols}&times;{footprint.rows} attach
                  </span>
                )}
                {hasOverhang && coverage && (
                  <span className="compat-coverage">
                    {coverage.cols}&times;{coverage.rows} covers
                  </span>
                )}
                {entry.adapters?.map((adapter) => (
                  <span key={adapter.sku} className="compat-adapter">
                    + {adapter.sku}
                  </span>
                ))}
                {isDisabled && reasonLabel(entry.reason) && (
                  <span className="compat-reason">{reasonLabel(entry.reason)}</span>
                )}
              </button>
              {!isDisabled && grid && footprint && (
                <button
                  className="btn btn-mini"
                  onClick={() =>
                    onAutoPlace(entry.sku, footprint.cols, footprint.rows, coverage)
                  }
                  disabled={!canAutoPlace}
                  title={
                    canAutoPlace
                      ? "Find the first free cell block and mount there"
                      : `No free ${footprint.cols}\u00d7${footprint.rows} block on this slot`
                  }
                >
                  auto-place
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
