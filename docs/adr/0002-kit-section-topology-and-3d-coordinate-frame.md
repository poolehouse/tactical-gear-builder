# Kit Section Multi-Face Topology and 3D-Ready Coordinate Frame

Every `MountSlot` on a Platform or mounted Component is assigned to a typed `KitSection` (`front`, `rear`, `left_side`, `right_side`, `sub_load`, `belt`) accompanied by a 3D-ready section plane descriptor (`normal`, `origin_3d`) so each 2D section view maps directly to a face of a future 3D rig mesh. Switching the active `KitSection` in the UI filters the visible slots and layers for that face without resetting or mutating the underlying `LoadoutDraft`, preserving all placed attachments and placard tools across the entire kit.
