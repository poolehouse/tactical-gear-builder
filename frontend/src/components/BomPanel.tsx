import type { DraftState, MountedItem } from "../types";

/** Bottom dock: selected-item inspector, the bill of materials, warnings,
 * and the raw engine evaluation JSON. */
export function BomPanel({
  state,
  selectedMount,
  onUnequip,
}: {
  state: DraftState | null;
  selectedMount: MountedItem | null;
  onUnequip: (mountId: string) => void;
}) {
  if (!state) return null;
  const { summary } = state;

  return (
    <section className="dock">
      {selectedMount && (
        <div className="inspector">
          <h2 className="pane-title">
            Inspector <span className="pane-count">// {selectedMount.mount_id}</span>
          </h2>
          <dl className="inspector-rows">
            <div>
              <dt>sku</dt>
              <dd>
                <code>{selectedMount.sku}</code>
              </dd>
            </div>
            <div>
              <dt>slot</dt>
              <dd>
                <code>{selectedMount.slot_id}</code>
              </dd>
            </div>
            {selectedMount.pals_position && (
              <div>
                <dt>position</dt>
                <dd>
                  col {selectedMount.pals_position[0]}, row{" "}
                  {selectedMount.pals_position[1]}
                </dd>
              </div>
            )}
            {selectedMount.pals_footprint && (
              <div>
                <dt>footprint</dt>
                <dd>
                  {selectedMount.pals_footprint.cols}&times;
                  {selectedMount.pals_footprint.rows} cells
                </dd>
              </div>
            )}
            {selectedMount.required_adapters.length > 0 && (
              <div>
                <dt>adapters</dt>
                <dd>
                  {selectedMount.required_adapters.map((sku) => (
                    <code key={sku}>{sku}</code>
                  ))}
                </dd>
              </div>
            )}
          </dl>
          <button className="btn btn-danger" onClick={() => onUnequip(selectedMount.mount_id)}>
            Unequip {selectedMount.mount_id}
            {selectedMount.child_mounts.length > 0 &&
              ` (+${selectedMount.child_mounts.length} child mounts)`}
          </button>
        </div>
      )}

      <div className="bom">
        <h2 className="pane-title">
          Bill of Materials{" "}
          <span className="pane-count">// {summary.bom.length} lines</span>
        </h2>
        {summary.bom.length === 0 ? (
          <p className="slot-hint">Nothing equipped yet.</p>
        ) : (
          <table className="bom-table">
            <thead>
              <tr>
                <th>qty</th>
                <th>sku</th>
                <th>name</th>
                <th>kind</th>
              </tr>
            </thead>
            <tbody>
              {summary.bom.map((line) => (
                <tr key={line.sku} className={line.is_adapter ? "bom-adapter" : ""}>
                  <td>{line.quantity}</td>
                  <td>
                    <code>{line.sku}</code>
                  </td>
                  <td>{line.name}</td>
                  <td>{line.is_adapter ? "adapter" : "component"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {summary.warnings.length > 0 && (
          <ul className="warning-list">
            {summary.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        )}
      </div>

      <details className="raw-json">
        <summary>
          <span className="prompt">$</span> engine evaluation JSON
        </summary>
        <pre>{JSON.stringify(summary, null, 2)}</pre>
      </details>
    </section>
  );
}
