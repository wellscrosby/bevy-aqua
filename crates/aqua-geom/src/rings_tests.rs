use super::*;

#[test]
fn centre_and_ring_have_crest_tile_counts() {
    assert_eq!(tile_layout(0).len(), CENTER_TILE_COUNT);
    for lod in 1..LOD_COUNT {
        assert_eq!(tile_layout(lod).len(), RING_TILE_COUNT);
    }
}

#[test]
fn only_largest_ring_uses_outer_skirts() {
    for lod in 0..LOD_COUNT - 1 {
        assert!(
            tile_layout(lod)
                .iter()
                .all(|tile| !matches!(tile.patch, Patch::FatXOuter | Patch::FatXZOuter))
        );
    }
    assert!(
        tile_layout(LOD_COUNT - 1)
            .iter()
            .all(|tile| matches!(tile.patch, Patch::FatXOuter | Patch::FatXZOuter))
    );
}
