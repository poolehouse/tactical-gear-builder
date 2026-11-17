import type { DraftState, KitSection, SlotAddress } from "../types";
import { prettifyEnum, sectionOfSlotId } from "../types";

const SECTION_SHORT: Record<Exclude<KitSection, "all">, string> = {
  front: "front",
  rear: "rear",
  left_side: "left",
  right_side: "right",
  sub_load: "sub-load",
  belt: "belt",
};

/** Slot addressing panel: platform slots plus child slots exposed by
 * mounted components (`m2.placard_front_pals` style addresses). */
export function SlotPanel({
  slots,
  selectedSlotAddress,
  state,
  selectedMountId,
  activeSection,
  onSelectSlot,
  onSelectMount,
}: {
  slots: SlotAddress[];
  selectedSlotAddress: string | null;
  state: DraftState | null;
  selectedMountId: string | null;
  activeSection: KitSection;
  onSelectSlot: (address: string) => void;
  onSelectMount: (mountId: string) => void;
}) {
  const matchesSection = (address: string): boolean => {
    if (activeSection === "all") return true;
    const sec = sectionOfSlotId(address, state?.mounted);
    if (activeSection === "sub_load") return sec === "sub_load" || sec === "belt";
    return sec === activeSection;
  };

  const visibleSlots = slots.filter((s) => matchesSection(s.address));
  const platformSlots = visibleSlots.filter(
    (slot) => slot.host.kind === "platform"
  );
  const childSlots = visibleSlots.filter(
    (slot) => slot.host.kind === "mounted"
  );
  const mounts = state?.mounted ?? [];

  return (
    <section className="pane slot-pane">
      <h2 className="pane-title">
        Mount Slots{" "}
        {activeSection !== "all" && (
          <span className="pane-count">
            // {activeSection.replace("_", " ")}
          </span>
        )}
      </h2>
      <h3 className="slot-group-title">Platform</h3>
      <ul className="slot-list">
        {platformSlots.map(({ slot, address }) => (
          <SlotRow
            key={address}
            address={address}
            slot={slot}
            sectionTag={SECTION_SHORT[sectionOfSlotId(address, state?.mounted)]}
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
                  sectionTag={
                    SECTION_SHORT[sectionOfSlotId(address, state?.mounted)]
                  }
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
      {mounts.length > 0 && (
        <>
          <h3 className="slot-group-title">Mounted items</h3>
          <ul className="slot-list">
            {mounts.map((item) => (
              <li key={item.mount_id}>
                <button
                  className={`slot-row mount-row${
                    item.mount_id === selectedMountId ? " is-selected" : ""
                  }`}
                  onClick={() => onSelectMount(item.mount_id)}
                  title={`${item.name} (${item.mount_id}) on ${item.slot_id}`}
                >
                  <span className="slot-row-main">
                    <span className="slot-id">{item.mount_id}</span>
                    <span className="slot-interface">
                      {item.is_adapter ? "adapter" : "component"}
                    </span>
                  </span>
                  <span className="slot-row-meta">
                    {item.name}
                    <span className="slot-host">
                      on {item.slot_id}
                      {item.child_mounts.length > 0
                        ? ` \u00b7 +${item.child_mounts.length} children`
                        : ""}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

function SlotRow({
  address,
  slot,
  sectionTag,
  hostName,
  selected,
  onSelect,
}: {
  address: string;
  slot: SlotAddress["slot"];
  sectionTag: string;
  hostName: string;
  selected: boolean;
  onSelect: (address: string) => void;
}) {
  const grid = slot.pals_grid;
  const occupied = grid
    ? grid.occupied.flat().filter(Boolean).length
    : 0;
  const covered = grid?.covered
    ? grid.covered.flat().filter(Boolean).length
    : 0;
  return (
    <li>
      <button
        className={`slot-row${selected ? " is-selected" : ""}`}
        onClick={() => onSelect(address)}
      >
        <span className="slot-row-main">
          <span className="slot-id">{slot.id}</span>
          <span className="slot-interface">
            <span className="slot-section-tag">{sectionTag}</span> &middot;{" "}
            {prettifyEnum(slot.interface)}
          </span>
        </span>
        <span className="slot-row-meta">
          {grid ? (
            <>
              {occupied}/{grid.cols * grid.rows} cells ({grid.cols}&times;
              {grid.rows} 1&times;1 cuts
              {covered > 0 ? `, ${covered} covered` : ""})
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
