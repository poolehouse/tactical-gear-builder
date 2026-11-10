// TypeScript mirrors of the `core-engine` serde DTOs served across the
// wasm boundary. Field names and casing match the Rust serialization
// exactly (snake_case keys; enums either snake_case or bare variant names).

export interface AnchorBox2D {
  x_pct: number;
  y_pct: number;
  width_pct: number;
  height_pct: number;
  z_index: number;
}

export interface PalsMatrix {
  cols: number;
  rows: number;
  /** Row-major: occupied[row][col] */
  occupied: boolean[][];
}

export interface MountSlot {
  id: string;
  label: string;
  interface: string;
  anchor: AnchorBox2D;
  pals_grid?: PalsMatrix;
}

export type PlatformCategory = "plate_carrier" | "belt" | "chest_rig" | "lbe";

export interface PlatformSummary {
  sku: string;
  name: string;
  category: PlatformCategory | null;
  slot_count: number;
}

export interface PlatformSpec {
  id: string;
  sku: string;
  name: string;
  category: PlatformCategory | null;
  plate_cuts: string[];
  sizes: string[];
  slots: MountSlot[];
}

export interface PlatformWithLiveState extends PlatformSpec {}

export interface PalsFootprint {
  cols: number;
  rows: number;
}

export interface MountedItem {
  mount_id: string;
  sku: string;
  name: string;
  is_adapter: boolean;
  slot_id: string;
  pals_position: [number, number] | null;
  pals_footprint: PalsFootprint | null;
  required_adapters: string[];
  serves_mount: string | null;
  anchor: AnchorBox2D;
  z_index: number;
  child_slots: MountSlot[];
  child_mounts: string[];
}

export type LayerKind = "platform" | "adapter" | "component";

export interface Layer2D {
  mount_id: string;
  sku: string;
  name: string;
  kind: LayerKind;
  z_index: number;
  anchor: AnchorBox2D;
  slot_id: string;
}

export interface BomLineItem {
  sku: string;
  name: string;
  quantity: number;
  is_adapter: boolean;
}

export interface LoadoutSummary {
  platform_sku: string;
  platform_name: string;
  valid: boolean;
  layers_2d: Layer2D[];
  bom: BomLineItem[];
  warnings: string[];
  errors: string[];
}

export interface DraftState {
  summary: LoadoutSummary;
  platform: PlatformWithLiveState;
  mounted: MountedItem[];
}

export interface AdapterSummary {
  sku: string;
  name: string;
}

export interface ComponentCompatEntry {
  sku: string;
  name: string;
  kind: string;
  pals_footprint?: PalsFootprint;
  adapters?: AdapterSummary[];
  reason?: unknown;
}

export interface CompatReport {
  native_fit: ComponentCompatEntry[];
  requires_adapter: ComponentCompatEntry[];
  incompatible: ComponentCompatEntry[];
}

export interface OpResult {
  ok: boolean;
  error?: string;
}

// --- geometry helpers ------------------------------------------------------------

/** Compose a child anchor (relative to `parent`) into canvas space — the
 * TS mirror of `AnchorBox2D::compose` in core-engine. */
export function compose(parent: AnchorBox2D, child: AnchorBox2D): AnchorBox2D {
  return {
    x_pct: parent.x_pct + (child.x_pct * parent.width_pct) / 100,
    y_pct: parent.y_pct + (child.y_pct * parent.height_pct) / 100,
    width_pct: (child.width_pct * parent.width_pct) / 100,
    height_pct: (child.height_pct * parent.height_pct) / 100,
    z_index: parent.z_index + child.z_index,
  };
}

/** Whether a footprint fits at (col, row) on a live PALS grid. */
export function gridCanFit(
  grid: PalsMatrix,
  col: number,
  row: number,
  cols: number,
  rows: number
): boolean {
  if (col < 0 || row < 0 || col + cols > grid.cols || row + rows > grid.rows) {
    return false;
  }
  for (let r = row; r < row + rows; r++) {
    for (let c = col; c < col + cols; c++) {
      if (grid.occupied[r][c]) return false;
    }
  }
  return true;
}

/** First free cell for a footprint, scanning left-to-right then top-to-bottom. */
export function firstFreeCell(
  grid: PalsMatrix,
  cols: number,
  rows: number
): [number, number] | null {
  for (let row = 0; row + rows <= grid.rows; row++) {
    for (let col = 0; col + cols <= grid.cols; col++) {
      if (gridCanFit(grid, col, row, cols, rows)) return [col, row];
    }
  }
  return null;
}

// --- display helpers ---------------------------------------------------------------

/** "CryeAvsFlap" -> "Crye Avs Flap" (interface enums serialize as bare
 * variant names). */
export function prettifyEnum(raw: string): string {
  return raw
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2");
}

/** The slot address the engine expects: platform slot ids pass through;
 * child slots are addressed as `<mount_id>.<child_slot_id>`. */
export interface SlotAddress {
  /** Engine-facing address (what `equip` takes). */
  address: string;
  /** The slot definition (compatibility + anchor + live grid). */
  slot: MountSlot;
  /** Where this slot lives — the platform or a mounted item. */
  host: { kind: "platform" } | { kind: "mounted"; mount_id: string };
}
