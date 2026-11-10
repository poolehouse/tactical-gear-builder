import type { DraftState, SlotAddress } from "../types";
import { prettifyEnum } from "../types";

/** Slot addressing panel: platform slots plus child slots exposed by
 * mounted components (`m2.placard_front_pals` style addresses). */
export function SlotPanel({
  slots,
  selectedSlotAddress,
  state,
  onSelectSlot,
}: {
  slots: SlotAddress[];
  selectedSlotAddress: string | null;
  state: DraftState | null;
  onSelectSlot: (address: string) => void;
}) {
  const platformSlots = slots.filter((slot) => slot.host.kind === "platform");
  const childSlots = slots.filter((slot) => slot.host.kind === "mounted");

  return (
    <section className="pane slot-pane">
      <h2 className="pane-title">Mount Slots</h2>
      <h3 className="slot-group-title">Platform</h3>
      <ul className="slot-list">
        {platformSlots.map(({ slot, address }) => (
          <SlotRow
            key={address}
            address={address}
            slot={slot}
            hostName={state?.platform.name ?? ""}
            selected={address === selectedSlotAddress}
            onSelect={onSelectSlot}
          />
        ))}
      </ul>
      {childSlots.length > 0 && (
        <>
          <h3 className="slot-group-title">On mounted components</h3>
          <ul className="slot-list">
            {childSlots.map(({ slot, address, host }) => {
              const hostMountId = host.kind === "mounted" ? host.mount_id : "";
              const hostItem = state?.mounted.find(
                (item) => item.mount_id === hostMountId
              );
              return (
                <SlotRow
                  key={address}
                  address={address}
                  slot={slot}
                  hostName={hostItem?.name ?? hostMountId}
                  selected={address === selectedSlotAddress}
                  onSelect={onSelectSlot}
                />
              );
            })}
          </ul>
        </>
      )}
      {childSlots.length === 0 && (
        <p className="slot-hint">
          Mounted placards and panels expose their own child slots here.
        </p>
      )}
    </section>
  );
}

function SlotRow({
  address,
  slot,
  hostName,
  selected,
  onSelect,
}: {
  address: string;
  slot: SlotAddress["slot"];
  hostName: string;
  selected: boolean;
  onSelect: (address: string) => void;
}) {
  const grid = slot.pals_grid;
  const occupied = grid
    ? grid.occupied.flat().filter(Boolean).length
    : 0;
  return (
    <li>
      <button
        className={`slot-row${selected ? " is-selected" : ""}`}
        onClick={() => onSelect(address)}
      >
        <span className="slot-row-main">
          <span className="slot-id">{slot.id}</span>
          <span className="slot-interface">{prettifyEnum(slot.interface)}</span>
        </span>
        <span className="slot-row-meta">
          {grid ? (
            <>
              {occupied}/{grid.cols * grid.rows} cells
            </>
          ) : (
            "anchor mount"
          )}
          <span className="slot-host">on {hostName}</span>
        </span>
      </button>
    </li>
  );
}
