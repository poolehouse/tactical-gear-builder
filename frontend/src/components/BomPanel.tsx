import type { DraftState, MountedItem, VendorOffer } from "../types";
import { CompliancePanel } from "./CompliancePanel";

/** Format integer US cents as a dollar string. */
const usd = (cents: number): string =>
  `$${(cents / 100).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;

/** Best in-stock vendor offer under the engine's documented deterministic
 * rule (lowest price wins, ties broken by vendor name in lexicographic
 * order; no in-stock offer -> null, i.e. the MSRP stands). Display-only
 * twin of `core_engine::pricing::best_offer` for rows the summary does not
 * precompute (the platform line). */
const bestOffer = (offers: VendorOffer[]): VendorOffer | null => {
  let best: VendorOffer | null = null;
  for (const offer of offers) {
    if (!offer.in_stock) continue;
    if (
      best === null ||
      offer.price_cents < best.price_cents ||
      (offer.price_cents === best.price_cents &&
        offer.vendor_name < best.vendor_name)
    ) {
      best = offer;
    }
  }
  return best;
};

/** Bottom dock: selected-item inspector, the priced bill of materials, the
 * regulation compliance report, warnings, and the raw engine evaluation
 * JSON. */
export function BomPanel({
  state,
  selectedMount,
  onUnequip,
  onSuggestedSku,
}: {
  state: DraftState | null;
  selectedMount: MountedItem | null;
  onUnequip: (mountId: string) => void;
  onSuggestedSku: (sku: string) => void;
}) {
  if (!state) return null;
  const { summary } = state;

  const platformOffer = bestOffer(state.platform.vendor_offers);
  const platformSavings = platformOffer
    ? Math.max(0, state.platform.msrp_cents - platformOffer.price_cents)
    : 0;
  const evaluatedColorway = state.colorway ?? state.platform.default_colorway;

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
            <div>
              <dt>msrp</dt>
              <dd>{usd(selectedMount.msrp_cents)}</dd>
            </div>
          </dl>
          <button className="btn btn-danger" onClick={() => onUnequip(selectedMount.mount_id)}>
            Unequip {selectedMount.mount_id}
            {selectedMount.child_mounts.length > 0 &&
              ` (+${selectedMount.child_mounts.length} child mounts)`}
          </button>
        </div>
      )}

      <div className="dock-main">
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
                  <th className="bom-num">msrp</th>
                  <th className="bom-num">best offer</th>
                  <th className="bom-num">saved</th>
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
                    <td className="bom-num">{usd(line.msrp_cents)}</td>
                    <td className="bom-num bom-vendor">
                      {line.best_offer
                        ? `${line.best_offer.vendor_name} @ ${usd(line.best_offer.price_cents)}`
                        : "\u2014 out of stock \u2014"}
                    </td>
                    <td className="bom-num bom-save">
                      {line.savings_cents > 0 ? `-${usd(line.savings_cents)}` : "\u2014"}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr className="bom-platform">
                  <td>1</td>
                  <td>
                    <code>{state.platform.sku}</code>
                  </td>
                  <td>{state.platform.name} (platform)</td>
                  <td>platform</td>
                  <td className="bom-num">{usd(state.platform.msrp_cents)}</td>
                  <td className="bom-num bom-vendor">
                    {platformOffer
                      ? `${platformOffer.vendor_name} @ ${usd(platformOffer.price_cents)}`
                      : "\u2014 out of stock \u2014"}
                  </td>
                  <td className="bom-num bom-save">
                    {platformSavings > 0 ? `-${usd(platformSavings)}` : "\u2014"}
                  </td>
                </tr>
                <tr className="bom-totals">
                  <td colSpan={4}>totals incl. platform</td>
                  <td className="bom-num">{usd(summary.total_msrp_cents)}</td>
                  <td className="bom-num">{usd(summary.total_best_price_cents)}</td>
                  <td className="bom-num bom-save">
                    -{usd(summary.total_savings_cents)}
                  </td>
                </tr>
              </tfoot>
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

        <CompliancePanel
          report={state.compliance ?? null}
          colorway={state.compliance ? evaluatedColorway : null}
          onSuggestedSku={onSuggestedSku}
        />
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
