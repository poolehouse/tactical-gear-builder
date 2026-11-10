import { useState } from "react";
import type {
  AnchorBox2D,
  DraftState,
  Layer2D,
  MountedItem,
  PalsFootprint,
  SlotAddress,
} from "../types";
import { compose, prettifyEnum } from "../types";

/**
 * The 2D paper-doll canvas. Everything is positioned by the engine's
 * percentage-space `AnchorBox2D` values, so the scene scales with the
 * container (aspect ratio 3:4, matching a torso canvas).
 */
export function LoadoutCanvas({
  state,
  slots,
  selectedSlot,
  selectedMountId,
  pendingSku,
  pendingFootprint,
  onSelectMount,
  onCellClick,
  onZoneClick,
}: {
  state: DraftState;
  slots: SlotAddress[];
  selectedSlot: SlotAddress | null;
  selectedMountId: string | null;
  pendingSku: string | null;
  pendingFootprint: PalsFootprint | null;
  onSelectMount: (mountId: string) => void;
  onCellClick: (slotAddress: string, col: number, row: number) => void;
  onZoneClick: (slotAddress: string) => void;
}) {
  const [hoverCell, setHoverCell] = useState<[number, number] | null>(null);

  const canvasAnchorOf = (slot: SlotAddress): AnchorBox2D => {
    if (slot.host.kind === "platform") return slot.slot.anchor;
    const hostMountId = slot.host.mount_id;
    const host = state.mounted.find((item) => item.mount_id === hostMountId);
    if (!host) return slot.slot.anchor;
    return compose(host.anchor, slot.slot.anchor);
  };

  const placing = pendingSku !== null && selectedSlot !== null;
  const gridSlot =
    placing && selectedSlot?.slot.pals_grid && pendingFootprint
      ? selectedSlot
      : null;

  const layers = state.summary.layers_2d;
  const mountedItems = new Map(state.mounted.map((item) => [item.mount_id, item]));

  return (
    <div className="canvas-wrap">
      <div className="canvas" role="img" aria-label="Loadout canvas">
        {/* base platform */}
        <div className="layer platform-silhouette" style={layerStyle(layers[0].anchor)}>
          <span className="platform-name">{state.platform.name}</span>
        </div>

        {/* slot zones (hit targets + hover labels) */}
        {slots.map((slot) => {
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
            >
              <span className="slot-zone-label">{slot.slot.id}</span>
            </div>
          );
        })}

        {/* PALS grid overlay on the selected slot while placing */}
        {gridSlot && gridSlot.slot.pals_grid && pendingFootprint && (() => {
          const grid = gridSlot.slot.pals_grid;
          const anchor = canvasAnchorOf(gridSlot);
          const cellW = anchor.width_pct / grid.cols;
          const cellH = anchor.height_pct / grid.rows;
          return Array.from({ length: grid.rows }, (_, row) =>
            Array.from({ length: grid.cols }, (_, col) => {
              const occupied = grid.occupied[row][col];
              const previewed =
                hoverCell !== null &&
                col >= hoverCell[0] &&
                col < hoverCell[0] + pendingFootprint.cols &&
                row >= hoverCell[1] &&
                row < hoverCell[1] + pendingFootprint.rows;
              return (
                <button
                  key={`${col}-${row}`}
                  className={`pals-cell${occupied ? " is-occupied" : ""}${
                    previewed ? " is-preview" : ""
                  }`}
                  style={{
                    left: `${anchor.x_pct + cellW * col}%`,
                    top: `${anchor.y_pct + cellH * row}%`,
                    width: `${cellW}%`,
                    height: `${cellH}%`,
                  }}
                  onMouseEnter={() => setHoverCell([col, row])}
                  onMouseLeave={() => setHoverCell(null)}
                  onFocus={() => setHoverCell([col, row])}
                  onBlur={() => setHoverCell(null)}
                  onClick={() => onCellClick(gridSlot.address, col, row)}
                  aria-label={`PALS cell ${col},${row}${occupied ? " (occupied)" : ""}`}
                />
              );
            })
          );
        })()}

        {/* mounted layers (adapters, components) */}
        {layers.slice(1).map((layer) => (
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
        <span><span className="legend-swatch legend-adapter" /> adapter hardware</span>
        <span><span className="legend-swatch legend-component" /> component</span>
        <span><span className="legend-swatch legend-child" /> child mount</span>
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
  return (
    <button
      className={`layer mount-layer kind-${layer.kind}${isChild ? " is-child" : ""}${
        selected ? " is-selected" : ""
      }`}
      style={layerStyle(layer.anchor)}
      onClick={() => onSelect(layer.mount_id)}
      title={`${layer.name} (${layer.mount_id}) — ${layer.slot_id}`}
    >
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
