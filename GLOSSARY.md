# Tactical Gear Builder

A compatibility engine and 2D/3D-ready loadout builder for modular tactical platforms (plate carriers, battle belts, chest rigs, LBE), evaluating interface bridges, dual-footprint MOLLE/PALS occupancy (attachment vs. material coverage), section-scoped spatial views, placard tool loadouts, vendor pricing, and regulation/role compliance.

## Language

### Hardware & Topology

**Platform**:
The base load-bearing garment (e.g. a plate carrier, battle belt, chest rig, or LBE) that anchors a loadout and exposes root Mount Slots across one or more Kit Sections.
_Avoid_: Carrier, base rig, garment, vest

**Kit Section**:
A discrete physical face or zone of the kit (e.g. Front, Rear, Left Cummerbund, Right Cummerbund, Dangler / Sub-Load, Belt) that the user can click between in the builder while preserving all already-placed Mounts across every other section.
_Avoid_: Page, tab view, side

**Component**:
A user-equipable piece of gear (placard, magazine pouch, IFAK, radio pouch, back panel) with a declared mounting interface, Attachment Footprint, and Material Coverage Footprint.
_Avoid_: Item, attachment, accessory, widget

**Placard Tool**:
A mission-specific tool or munition (breach charge, Halligan tool, Good Guy BRB tool, radio, rifle magazine, trauma insert) affixed to or carried inside a mounted placard, back panel, or pouch cell.
_Avoid_: Sub-item, prop, inventory item

**Adapter Bridge**:
Auto-managed hardware (such as an AXL AVP) automatically injected into a Loadout Draft when a Component's interface requires a single-hop conversion to mate with a target Mount Slot, and automatically garbage-collected when the served Component is unequipped.
_Avoid_: Converter, shim, connector

**Mount Slot**:
A named attachment point on a Platform or on a mounted Component, typed by its physical interface and positioned by an Anchor Box; it is either a PALS Grid Slot or an Anchor Slot.
_Avoid_: Socket, port, drop zone, mount point

**Slot Address**:
The canonical identifier targeting a Mount Slot: a bare slot ID for a Platform slot (e.g. `front_placard`) or `<mount_id>.<child_slot_id>` for a Child Slot on a mounted Component (e.g. `m2.placard_front_pals`).
_Avoid_: Slot key, slot path, slot ref

**Child Slot**:
A Mount Slot exposed by an already-mounted Component (such as the front PALS field or internal tool/insert bays on a placard), whose coordinates compose relative to its host Mount.
_Avoid_: Sub-slot, nested slot, secondary slot

**Mount**:
A single equipped instance of a Component, Placard Tool, or Adapter Bridge inside a Loadout Draft, assigned a monotonic ID (`m1`, `m2`, ...) for the lifetime of the draft.
_Avoid_: Placement record, equipped item, layer instance

### Spatial & Placement Model

**PALS Grid**:
A 2D matrix of `1x1` MOLLE/PALS cuts (columns = vertical PALS channels, rows = horizontal webbing rows) on a Mount Slot that tracks cell-by-cell occupancy.
_Avoid_: MOLLE table, cell matrix, slot grid

**Attachment Footprint**:
The `cols x rows` rectangle of `1x1` PALS Grid cells that a Component's physical mounting straps/clips weave into and lock onto.
_Avoid_: Size, strap size, mount box

**Material Coverage Footprint**:
The `cols x rows` (and optional directional offset / overhang) area of the PALS Grid physically covered or occluded by the body/fabric of the mounted Component, which may exceed the Attachment Footprint (e.g. a wide GP pouch or tool carrier that attaches via 2x2 PALS channels but physically overhangs 4x3 cells).
_Avoid_: Visual size, bounding box, bloat box

**Anchor Cell**:
The top-left `(col, row)` PALS cell where a Component's Attachment Footprint is anchored (via drag-and-drop or click-to-place).
_Avoid_: Origin cell, target cell, click cell

**Anchor Slot**:
A non-grid Mount Slot (such as a Crye AVS flap or G-hook placard slot) where a Component mounts to the whole slot zone rather than a PALS cell coordinate.
_Avoid_: Zone slot, non-PALS slot, point mount

**Pending Placement**:
The transient UI state where a user is dragging or has selected a compatible Component SKU to mount on a Mount Slot, awaiting drop or click on a valid Anchor Cell or Anchor Slot zone.
_Avoid_: Active tool, selected SKU

**Blocked Anchor**:
A PALS cell that cannot serve as the Anchor Cell for a Pending Placement because either the Component's Attachment Footprint would overflow the PALS Grid or collide with occupied attachment cells, or its Material Coverage Footprint would collide with neighboring mounted gear.
_Avoid_: Invalid cell, disabled cell, out-of-bounds cell

**Occluded Mount**:
A Mount whose 2D canvas layer is visually or hit-target covered by a higher-z-index Mount or by Material Coverage overhang.
_Avoid_: Hidden item, buried layer

### Evaluation & Compliance

**Loadout Draft**:
A stateful, deterministic assembly of a Platform and its sequence of Mounts across all Kit Sections, producing an ordered render stack, a priced BOM, and an optional Compliance Report.
_Avoid_: Build session, cart, configuration

**Compliance Context**:
The optional triple of Colorway, Regulation Policy, and Operational Role under which a Loadout Draft is evaluated; when no policy or role is active, the chosen Colorway (or Platform default) still governs visual presentation.
_Avoid_: Filter settings, validation mode
