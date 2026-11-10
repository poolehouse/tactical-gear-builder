import { useMemo, useState } from "react";
import type { PlatformCategory, PlatformSummary } from "../types";

const CATEGORY_LABELS: Record<PlatformCategory, string> = {
  plate_carrier: "Plate Carriers",
  belt: "Belts",
  chest_rig: "Chest Rigs",
  lbe: "LBE",
};

const CATEGORY_ORDER: PlatformCategory[] = [
  "plate_carrier",
  "belt",
  "chest_rig",
  "lbe",
];

export function PlatformPicker({
  platforms,
  platformSku,
  onSelect,
}: {
  platforms: PlatformSummary[];
  platformSku: string | null;
  onSelect: (sku: string) => void;
}) {
  const [query, setQuery] = useState("");

  const grouped = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = platforms.filter(
      (platform) =>
        !needle ||
        platform.name.toLowerCase().includes(needle) ||
        platform.sku.includes(needle)
    );
    return CATEGORY_ORDER.map((category) => ({
      category,
      platforms: filtered.filter((platform) => platform.category === category),
    })).filter((group) => group.platforms.length > 0);
  }, [platforms, query]);

  return (
    <section className="pane picker-pane">
      <h2 className="pane-title">
        Platform <span className="pane-count">// {platforms.length}</span>
      </h2>
      <input
        className="search"
        type="search"
        placeholder="filter platforms&hellip;"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        aria-label="Filter platforms"
      />
      {grouped.map(({ category, platforms }) => (
        <div className="picker-group" key={category}>
          <h3 className="picker-group-title">{CATEGORY_LABELS[category]}</h3>
          <ul className="picker-list">
            {platforms.map((platform) => (
              <li key={platform.sku}>
                <button
                  className={`picker-item${
                    platform.sku === platformSku ? " is-selected" : ""
                  }`}
                  onClick={() => onSelect(platform.sku)}
                >
                  <span className="picker-name">{platform.name}</span>
                  <span className="picker-meta">{platform.slot_count} slots</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}
