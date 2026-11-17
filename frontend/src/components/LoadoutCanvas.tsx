import { useState } from "react";
import type {
  AnchorBox2D,
  DraftState,
  KitSection,
  Layer2D,
  MaterialCoverage,
  MountedItem,
  PalsFootprint,
  SlotAddress,
} from "../types";
import { compose, gridCanFit, prettifyEnum, sectionOfSlotId } from "../types";

const SECTION_TABS: { id: KitSection; label: string }[] = [
  { id: "all", label: "All Sections" },
  { id: "front", label: "Front" },
  { id: "rear", label: "Rear" },
  { id: "left_side", label: "Left Side" },
  { id: "right_side", label: "Right Side" },
  { id: "sub_load", label: "Sub-Load / Belt" },
];

/**
 * The 2D paper-doll canvas with Kit Section switching, 1x1 PALS grid cuts,
 * dual-footprint preview (attachment vs material coverage overhang), and
 * drag-and-drop placement.
 */
export function LoadoutCanvas({
  state,
  slots,
  selectedSlot,
  selectedMountId,
  activeSection,
  pendingSku,
  pendingFootprint,
  pendingCoverage,
  onSelectSection,
  onSelectMount,
  onCellClick,
  onZoneClick,
  onDropOnCell,
  onDropOnZone,
}: {
  state: DraftState;
  slots: SlotAddress[];
  selectedSlot: SlotAddress | null;
  selectedMountId: string | null;
  activeSection: KitSection;
  pendingSku: string | null;
  pendingFootprint: PalsFootprint | null;
  pendingCoverage: MaterialCoverage | null;
  onSelectSection: (section: KitSection) => void;
  onSelectMount: (mountId: string | null) => void;
  onCellClick: (slotAddress: string, col: number, row: number) => void;
  onZoneClick: (slotAddress: string) => void;
  onDropOnCell: (slotAddress: string, sku: string, col: number, row: number) => void;
  onDropOnZone: (slotAddress: string, sku: string) => void;
}) {
  const [hoverCell, setHoverCell] = useState<[number, number] | null>(null);

  const canvasAnchorOf = (slot: SlotAddress): AnchorBox2D => {
    if (slot.host.kind === "platform") return slot.slot.anchor;
    const hostMountId = slot.host.mount_id;
    const host = state.mounted.find((item) => item.mount_id === hostMountId);
    if (!host) return slot.slot.anchor;
    return compose(host.anchor, slot.slot.anchor);
  };

  const matchesActiveSection = (slotId: string): boolean => {
    if (activeSection === "all") return true;
    const sec = sectionOfSlotId(slotId, state.mounted);
    if (activeSection === "sub_load") {
      return sec === "sub_load" || sec === "belt";
    }
    return sec === activeSection;
  };

  const visibleSlots = slots.filter((s) => matchesActiveSection(s.address));

  const placing = pendingSku !== null && selectedSlot !== null;
  const gridSlot = selectedSlot?.slot.pals_grid ? selectedSlot : null;

  const layers = state.summary.layers_2d;
  const mountedItems = new Map(state.mounted.map((item) => [item.mount_id, item]));
  const visibleLayers = layers
    .slice(1)
    .filter((layer) => matchesActiveSection(layer.slot_id));

  return (
    <div className="canvas-wrap">
      <div className="section-tabs" role="tablist" aria-label="Kit sections">
        {SECTION_TABS.map((tab) => {
          const count =
            tab.id === "all"
              ? state.mounted.filter((m) => !m.is_adapter).length
              : state.mounted.filter((m) => {
                  if (m.is_adapter) return false;
                  const s = sectionOfSlotId(m.slot_id, state.mounted);
                  return tab.id === "sub_load"
                    ? s === "sub_load" || s === "belt"
                    : s === tab.id;
                }).length;
          return (
            <button
              key={tab.id}
              role="tab"
              aria-selected={activeSection === tab.id}
              className={`section-tab${
                activeSection === tab.id ? " is-active" : ""
              }`}
              onClick={() => onSelectSection(tab.id)}
            >
              <span>{tab.label}</span>
              {count > 0 && <span className="section-badge">{count}</span>}
            </button>
          );
        })}
      </div>

      <div
        className="canvas"
        role="img"
        aria-label="Loadout canvas"
        data-section={activeSection}
        onClick={(event) => {
          if ((event.target as HTMLElement).closest(".mount-layer")) return;
          onSelectMount(null);
        }}
      >
        {/* base platform */}
        <div
          className="layer platform-silhouette"
          data-colorway={state.colorway ?? state.platform.default_colorway}
          style={layerStyle(layers[0].anchor)}
        >
          <span className="platform-name">
            {state.platform.name}
            {activeSection !== "all" ? ` // ${activeSection.replace("_", " ")}` : ""}
          </span>
        </div>

        {/* slot zones (hit targets + hover labels) */}
        {visibleSlots.map((slot) => {
          const anchor = canvasAnchorOf(slot);
          const isChild = slot.host.kind === "mounted";
          const isSelected = selectedSlot?.address === slot.address;
          const isTarget = placing && isSelected;
          return (
            <div
              key={slot.address}
              className={`slot-zone${isSelected ? " is-selected" : ""}${
                isChild ? " is-child" : ""
              }${isTarget ? " is-target" : ""}`}
              style={layerStyle(anchor)}
              title={`${slot.slot.label} — ${prettifyEnum(slot.slot.interface)}`}
              onClick={() => {
                if (isTarget && !gridSlot) onZoneClick(slot.address);
              }}
              onDragOver={(event) => {
                if (isTarget && !gridSlot) {
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "copy";
                }
              }}
              onDrop={(event) => {
                if (isTarget && !gridSlot) {
                  event.preventDefault();
                  const sku =
                    event.dataTransfer.getData("text/plain") || pendingSku;
                  if (sku) onDropOnZone(slot.address, sku);
                }
              }}
            >
              <span className="slot-zone-label">{slot.slot.id}</span>
            </div>
          );
        })}

        {/* PALS grid overlay on the selected slot (always shows 1x1 MOLLE cuts; interactive when placing/dragging) */}
        {gridSlot &&
          gridSlot.slot.pals_grid &&
          (() => {
            const grid = gridSlot.slot.pals_grid;
            const anchor = canvasAnchorOf(gridSlot);
            const cellW = anchor.width_pct / grid.cols;
            const cellH = anchor.height_pct / grid.rows;
            const attachCols = pendingFootprint?.cols ?? 1;
            const attachRows = pendingFootprint?.rows ?? 1;
            const covCols = pendingCoverage?.cols ?? attachCols;
            const covRows = pendingCoverage?.rows ?? attachRows;
            const covOffCol = pendingCoverage?.offset_col ?? 0;
            const covOffRow = pendingCoverage?.offset_row ?? 0;
            const isArmed = placing && pendingFootprint !== null;

            const hoverCanPlace =
              isArmed &&
              hoverCell !== null &&
              gridCanFit(
                grid,
                hoverCell[0],
                hoverCell[1],
                attachCols,
                attachRows,
                pendingCoverage
              );

            return Array.from({ length: grid.rows }, (_, row) =>
              Array.from({ length: grid.cols }, (_, col) => {
                const occupied = grid.occupied[row][col];
                const covered = grid.covered?.[row]?.[col] ?? false;
                const canPlace = isArmed
                  ? !occupied &&
                    gridCanFit(
                      grid,
                      col,
                      row,
                      attachCols,
                      attachRows,
                      pendingCoverage
                    )
                  : !occupied;
                const inAttachPreview =
                  isArmed &&
                  hoverCell !== null &&
                  col >= hoverCell[0] &&
                  col < hoverCell[0] + attachCols &&
                  row >= hoverCell[1] &&
                  row < hoverCell[1] + attachRows;
                const inCoveragePreview =
                  isArmed &&
                  hoverCell !== null &&
                  !inAttachPreview &&
                  col >= hoverCell[0] + covOffCol &&
                  col < hoverCell[0] + covOffCol + covCols &&
                  row >= hoverCell[1] + covOffRow &&
                  row < hoverCell[1] + covOffRow + covRows;

                return (
                  <button
                    key={`${col}-${row}`}
                    type="button"
                    className={`pals-cell${!isArmed ? " is-idle" : ""}${
                      occupied ? " is-occupied" : ""
                    }${covered ? " is-covered" : ""}${
                      isArmed && !occupied && !canPlace ? " is-blocked" : ""
                    }${inAttachPreview && hoverCanPlace ? " is-preview" : ""}${
                      inCoveragePreview && hoverCanPlace
                        ? " is-preview-coverage"
                        : ""
                    }${
                      isArmed &&
                      (inAttachPreview || inCoveragePreview) &&
                      !occupied &&
                      !hoverCanPlace
                        ? " is-preview-invalid"
                        : ""
                    }`}
                    style={{
                      left: `${anchor.x_pct + cellW * col}%`,
                      top: `${anchor.y_pct + cellH * row}%`,
                      width: `${cellW}%`,
                      height: `${cellH}%`,
                    }}
                    onMouseEnter={() => {
                      if (isArmed) setHoverCell([col, row]);
                    }}
                    onMouseLeave={() => setHoverCell(null)}
                    onFocus={() => {
                      if (isArmed) setHoverCell([col, row]);
                    }}
                    onBlur={() => setHoverCell(null)}
                    onDragOver={(event) => {
                      event.preventDefault();
                      setHoverCell([col, row]);
                      event.dataTransfer.dropEffect = canPlace ? "copy" : "none";
                    }}
                    onDrop={(event) => {
                      event.preventDefault();
                      setHoverCell(null);
                      const sku =
                        event.dataTransfer.getData("text/plain") || pendingSku;
                      if (!occupied && sku) {
                        onDropOnCell(gridSlot.address, sku, col, row);
                      }
                    }}
                    onClick={() => {
                      if (isArmed && canPlace) {
                        onCellClick(gridSlot.address, col, row);
                      }
                    }}
                    aria-disabled={isArmed ? !canPlace : occupied}
                    title={
                      occupied
                        ? covered
                          ? `Cell (${col}, ${row}) — covered by adjacent item material overhang`
                          : `Cell (${col}, ${row}) — occupied`
                        : !isArmed
                          ? `1\u00d71 MOLLE cut (${col}, ${row}) — pick or drag a component here`
                          : canPlace
                            ? `Place at (${col}, ${row})`
                            : `${attachCols}\u00d7${attachRows} footprint does not fit here`
                    }
                    aria-label={`PALS cell ${col},${row}${
                      occupied
                        ? " (occupied)"
                        : isArmed && !canPlace
                          ? " (footprint does not fit)"
                          : ""
                    }`}
                  />
                );
              })
            );
          })()}

        {/* mounted layers (adapters, components) */}
        {visibleLayers.map((layer) => (
          <MountLayer
            key={layer.mount_id}
            layer={layer}
            item={mountedItems.get(layer.mount_id)}
            selected={layer.mount_id === selectedMountId}
            onSelect={onSelectMount}
          />
        ))}
      </div>

      <div className="canvas-legend">
        <span>
          <span className="legend-swatch legend-adapter" /> adapter hardware
        </span>
        <span>
          <span className="legend-swatch legend-component" /> component
        </span>
        <span>
          <span className="legend-swatch legend-child" /> child / tool mount
        </span>
        <span>
          <span className="legend-swatch legend-coverage" /> material coverage
        </span>
      </div>
    </div>
  );
}

function MountLayer({
  layer,
  item,
  selected,
  onSelect,
}: {
  layer: Layer2D;
  item: MountedItem | undefined;
  selected: boolean;
  onSelect: (mountId: string) => void;
}) {
  const isChild = item ? item.z_index >= 30 : false;
  const attach = item?.pals_footprint;
  const cov = item?.material_coverage;
  const hasOverhang = Boolean(
    attach && cov && (cov.cols !== attach.cols || cov.rows !== attach.rows)
  );
  return (
    <button
      className={`layer mount-layer kind-${layer.kind}${
        isChild ? " is-child" : ""
      }${hasOverhang ? " has-overhang" : ""}${selected ? " is-selected" : ""}`}
      style={layerStyle(layer.anchor)}
      onClick={() => onSelect(layer.mount_id)}
      title={`${layer.name} (${layer.mount_id}) — ${layer.slot_id}${
        hasOverhang && attach && cov
          ? ` [${attach.cols}\u00d7${attach.rows} attach, ${cov.cols}\u00d7${cov.rows} material coverage]`
          : ""
      }`}
    >
      {hasOverhang && attach && cov && (
        <span
          className="material-overhang-box"
          style={{
            width: `${(cov.cols / attach.cols) * 100}%`,
            height: `${(cov.rows / attach.rows) * 100}%`,
          }}
          aria-hidden="true"
        />
      )}
      <span className="mount-label">{shortName(layer.name)}</span>
      <span className="mount-id">{layer.mount_id}</span>
    </button>
  );
}

function layerStyle(anchor: AnchorBox2D): React.CSSProperties {
  return {
    left: `${anchor.x_pct}%`,
    top: `${anchor.y_pct}%`,
    width: `${anchor.width_pct}%`,
    height: `${anchor.height_pct}%`,
    zIndex: anchor.z_index,
  };
}

function shortName(name: string): string {
  const words = name.split(" ");
  if (words.length <= 3) return name;
  return `${words.slice(0, 3).join(" ")}\u2026`;
}
