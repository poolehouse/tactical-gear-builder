import type { CompatReport, SlotAddress } from "../types";
import { prettifyEnum } from "../types";

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
 * classified by the engine into native fits, adapter-bridged mounts, and
 * hard incompatibilities.
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
  onAutoPlace: (sku: string, cols: number, rows: number) => void;
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

  const grid = selectedSlot.slot.pals_grid !== undefined;

  return (
    <section className="pane browser-pane">
      <h2 className="pane-title">
        Components <span className="pane-count">// for {selectedSlot.address}</span>
      </h2>

      {pendingSku && (
        <div className="pending-banner">
          <span>
            Placing <code>{pendingSku}</code> —{" "}
            {grid
              ? "click a cell on the highlighted slot."
              : "click the highlighted slot."}
          </span>
          <button className="btn btn-ghost" onClick={onClearPending}>
            Cancel
          </button>
        </div>
      )}

      <CompatGroup
        title="Native fit"
        note="mounts directly"
        entries={report.native_fit}
        variant="native"
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
        entries={report.incompatible}
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
  variant: "native" | "adapter" | "incompatible";
  pendingSku: string | null;
  grid: boolean;
  onPickComponent: (sku: string) => void;
  onAutoPlace: (sku: string, cols: number, rows: number) => void;
}) {
  if (entries.length === 0) return null;
  return (
    <div className={`compat-group compat-${variant}`}>
      <h3 className="compat-title">
        {title} <span className="compat-note">// {note}</span>
      </h3>
      <ul className="compat-list">
        {entries.map((entry) => {
          const footprint = entry.pals_footprint;
          return (
            <li key={entry.sku}>
              <button
                className={`compat-entry${pendingSku === entry.sku ? " is-pending" : ""}`}
                onClick={() => onPickComponent(entry.sku)}
                disabled={variant === "incompatible"}
              >
                <span className="compat-name">{entry.name}</span>
                <span className="compat-kind">{prettifyEnum(entry.kind)}</span>
                {footprint && (
                  <span className="compat-footprint">
                    {footprint.cols}&times;{footprint.rows} cells
                  </span>
                )}
                {entry.adapters?.map((adapter) => (
                  <span key={adapter.sku} className="compat-adapter">
                    + {adapter.sku}
                  </span>
                ))}
                {variant === "incompatible" && reasonLabel(entry.reason) && (
                  <span className="compat-reason">{reasonLabel(entry.reason)}</span>
                )}
              </button>
              {variant !== "incompatible" && grid && footprint && (
                <button
                  className="btn btn-mini"
                  onClick={() => onAutoPlace(entry.sku, footprint.cols, footprint.rows)}
                  title="Find the first free cell block and mount there"
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
