import type {
  CapabilityTag,
  Colorway,
  ComplianceIssue,
  ComplianceRecommendation,
  ComplianceReport,
  ComplianceRuleCode,
} from "../types";

/** "DisallowedColorway" -> "Disallowed Colorway" (unit variants are bare
 *  strings, data variants are `{ VariantName: payload }`). */
export function ruleCodeName(code: ComplianceRuleCode): string {
  const raw = typeof code === "string" ? code : Object.keys(code)[0];
  return raw.replace(/([a-z0-9])([A-Z])/g, "$1 $2");
}

/** Human label for a typed capability tag. */
export function capabilityLabel(tag: CapabilityTag): string {
  if (typeof tag === "string") return tag;
  if ("RifleMagCapacity" in tag) return `RifleMagCapacity(${tag.RifleMagCapacity})`;
  return Object.keys(tag)[0];
}

/** One-line payload summary for the data-carrying rule codes, or null for
 *  unit variants. */
export function ruleCodeDetail(code: ComplianceRuleCode): string | null {
  if (typeof code === "string") return null;
  if ("DisallowedColorway" in code) {
    const { chosen, allowed } = code.DisallowedColorway;
    return `chosen ${colorwayLabel(chosen)} · allowed ${allowed.map(colorwayLabel).join(", ")}`;
  }
  if ("InsufficientRifleMagCapacity" in code) {
    const { minimum, found } = code.InsufficientRifleMagCapacity;
    return `minimum ${minimum} · carried ${found}`;
  }
  if ("RecommendRifleMagCapacity" in code) {
    const { recommended, found } = code.RecommendRifleMagCapacity;
    return `recommended ${recommended} · carried ${found}`;
  }
  if ("MissingCapability" in code) {
    return `missing ${capabilityLabel(code.MissingCapability.capability)}`;
  }
  if ("MissingAnyCapability" in code) {
    const caps = code.MissingAnyCapability.capabilities.map(capabilityLabel);
    return `missing any of ${caps.join(", ")}`;
  }
  if ("UnsupportedPlatformCategory" in code) {
    const { allowed, found } = code.UnsupportedPlatformCategory;
    return `allowed ${allowed.join(", ")} · found ${found ?? "none"}`;
  }
  if ("RecommendPlatformCategory" in code) {
    const { suggested, found } = code.RecommendPlatformCategory;
    return `suggested ${suggested.join(", ")} · found ${found ?? "none"}`;
  }
  if ("ProneProfileStacking" in code) {
    return `on ${code.ProneProfileStacking.slot_id}`;
  }
  return null;
}

/** "coyote_brown" -> "Coyote Brown". */
export function colorwayLabel(colorway: Colorway): string {
  return colorway
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** Regulation & role compliance report: status badge, blocking violations,
 * non-blocking recommendations, and suggested-SKU chips that place the
 * component for you. */
export function CompliancePanel({
  report,
  colorway,
  onSuggestedSku,
}: {
  report: ComplianceReport | null;
  /** The colorway the report was evaluated under (platform default when
   * the user never picked one explicitly). */
  colorway: Colorway | null;
  onSuggestedSku: (sku: string) => void;
}) {
  return (
    <div className="compliance pane">
      <h2 className="pane-title">
        Compliance{" "}
        {report ? (
          <span className="pane-count">// {colorwayLabel(colorway ?? "multicam")}</span>
        ) : null}
      </h2>

      {!report ? (
        <p className="slot-hint">
          No regulation policy or role selected — pick one in the context bar
          (or load a role preset) to validate this build against military /
          LE regulations.
        </p>
      ) : (
        <>
          <StatusBadge report={report} />
          {report.violations.length > 0 && (
            <ul className="issue-list">
              {report.violations.map((issue, index) => (
                <IssueRow
                  key={`${ruleCodeName(issue.code)}-${index}`}
                  issue={issue}
                  severity="violation"
                  onSuggestedSku={onSuggestedSku}
                />
              ))}
            </ul>
          )}
          {report.recommendations.length > 0 && (
            <ul className="issue-list">
              {report.recommendations.map((rec, index) => (
                <IssueRow
                  key={`${ruleCodeName(rec.code)}-${index}`}
                  issue={rec}
                  severity="recommendation"
                  onSuggestedSku={onSuggestedSku}
                />
              ))}
            </ul>
          )}
          {report.violations.length === 0 && report.recommendations.length === 0 && (
            <p className="issue-clean">
              Clean build — no violations, no advisories under the selected
              policy and role.
            </p>
          )}
        </>
      )}
    </div>
  );
}

function StatusBadge({ report }: { report: ComplianceReport }) {
  const violations = report.violations.length;
  const advisories = report.recommendations.length;
  if (!report.compliant) {
    return (
      <p className={`compliance-status is-violation`}>
        Non-compliant — {violations} blocking violation{violations === 1 ? "" : "s"}
      </p>
    );
  }
  if (advisories > 0) {
    return (
      <p className="compliance-status is-advisory">
        Compliant — {advisories} advisory{advisories === 1 ? "" : "s"}
      </p>
    );
  }
  return <p className="compliance-status is-compliant">Compliant</p>;
}

function IssueRow({
  issue,
  severity,
  onSuggestedSku,
}: {
  issue: ComplianceIssue | ComplianceRecommendation;
  severity: "violation" | "recommendation";
  onSuggestedSku: (sku: string) => void;
}) {
  const detail = ruleCodeDetail(issue.code);
  return (
    <li className={`issue is-${severity}`}>
      <div className="issue-heading">
        <span className="issue-name">{ruleCodeName(issue.code)}</span>
        {detail && <span className="issue-detail">{detail}</span>}
      </div>
      <p className="issue-message">{issue.message}</p>
      {issue.suggested_skus.length > 0 && (
        <div className="issue-suggestions">
          <span className="issue-suggestions-label">fix with:</span>
          {issue.suggested_skus.map((sku) => (
            <button
              key={sku}
              className="suggestion-chip"
              onClick={() => onSuggestedSku(sku)}
              title={`Equip ${sku}`}
            >
              + {sku}
            </button>
          ))}
        </div>
      )}
    </li>
  );
}
