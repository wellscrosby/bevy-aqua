//! Crest-style concentric ocean tile geometry.

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

use crate::{LOD_COUNT, TILE_RESOLUTION};

const PATCH_HALF_WIDTH: f32 = 0.5;
const OUTER_TILE_OFFSET: f32 = 1.5;
const INNER_TILE_OFFSET: f32 = 0.5;
const TILE_AXIS: [f32; 4] = [-1.5, -0.5, 0.5, 1.5];
const CENTER_TILE_COUNT: usize = 16;
const RING_TILE_COUNT: usize = 12;
// Crest supports LOD indices 0 through 15. Its 16-slot capacity determines
// how far the final row extends toward the far plane.
const CREST_LOD_CAPACITY: usize = 16;
const CREST_EXTENT_BASE: f32 = 100.0;
const OUTER_EXTENT_MULTIPLIER: f32 = CREST_EXTENT_BASE * (CREST_LOD_CAPACITY - LOD_COUNT) as f32;
const REGULAR_EDGE_VERTICES: u32 = TILE_RESOLUTION + 1;
const INDICES_PER_QUAD: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
/// One reusable ocean-tile patch topology.
#[allow(missing_docs)] // variant names are self-describing
pub enum Patch {
    Interior,
    FatX,
    FatXSlimZ,
    FatXOuter,
    FatXZ,
    FatXZOuter,
    SlimX,
    SlimXZ,
    SlimXFatZ,
}

impl Patch {
    /// Every patch variant in build order.
    pub const ALL: [Self; 9] = [
        Self::Interior,
        Self::FatX,
        Self::FatXSlimZ,
        Self::FatXOuter,
        Self::FatXZ,
        Self::FatXZOuter,
        Self::SlimX,
        Self::SlimXZ,
        Self::SlimXFatZ,
    ];
}

/// One tile placement inside a ring layout.
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    /// Offset from the ring centre in tile units (scaled by lod_scale).
    pub offset: Vec2,
    pub patch: Patch,
    /// Y rotation in radians aligning fat/slim edges outward.
    pub rotation: f32,
}

#[derive(Clone, Copy)]
enum Edge {
    Slim,
    Regular,
    Fat,
}

impl Edge {
    fn vertex_count(self) -> u32 {
        match self {
            Self::Slim => REGULAR_EDGE_VERTICES - 1,
            Self::Regular => REGULAR_EDGE_VERTICES,
            Self::Fat => REGULAR_EDGE_VERTICES + 1,
        }
    }

    fn end(self, step: f32) -> f32 {
        match self {
            Self::Slim => PATCH_HALF_WIDTH - step,
            Self::Regular => PATCH_HALF_WIDTH,
            Self::Fat => PATCH_HALF_WIDTH + step,
        }
    }
}

#[derive(Clone, Copy)]
struct Edges {
    x: Edge,
    z: Edge,
    outer_x: bool,
    outer_z: bool,
}

/// Builds one reusable patch mesh.
///
/// Reimplementation of the approach in Crest `Scripts/OceanBuilder.cs` (`BuildOceanPatch`). Fat and slim
/// edge variants overlap without holes, while outer variants stretch one row
/// into a horizon skirt.
pub fn build_patch(patch: Patch) -> Mesh {
    let edges = patch_edges(patch);
    let columns = edges.x.vertex_count();
    let rows = edges.z.vertex_count();
    let step = (TILE_RESOLUTION as f32).recip();
    let end_x = edges.x.end(step);
    let end_z = edges.z.end(step);

    let mut positions = Vec::with_capacity((columns * rows) as usize);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for row in 0..rows {
        let fraction_z = row as f32 / (rows - 1) as f32;
        let mut z = -PATCH_HALF_WIDTH + (end_z + PATCH_HALF_WIDTH) * fraction_z;
        if edges.outer_z && row == rows - 1 {
            z *= OUTER_EXTENT_MULTIPLIER;
        }
        for column in 0..columns {
            let fraction_x = column as f32 / (columns - 1) as f32;
            let mut x = -PATCH_HALF_WIDTH + (end_x + PATCH_HALF_WIDTH) * fraction_x;
            if edges.outer_x && column == columns - 1 {
                x *= OUTER_EXTENT_MULTIPLIER;
            }
            positions.push([x, 0.0, z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([fraction_x, fraction_z]);
        }
    }

    let quad_count = (columns - 1) * (rows - 1);
    let mut indices = Vec::with_capacity(quad_count as usize * INDICES_PER_QUAD);
    for row in 0..rows - 1 {
        for column in 0..columns - 1 {
            add_quad(&mut indices, column, row, columns);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Returns Crest's 4x4 centre or 12-patch ring layout.
///
/// Reimplementation of the approach in Crest `Scripts/OceanBuilder.cs` (`CreateLOD`). Rotations keep the
/// fat/slim edge and outer-skirt sides pointed in the required direction.
pub fn tile_layout(lod: usize) -> Vec<Tile> {
    let capacity = if lod == 0 {
        CENTER_TILE_COUNT
    } else {
        RING_TILE_COUNT
    };
    let mut tiles = Vec::with_capacity(capacity);
    for &z in TILE_AXIS.iter().rev() {
        for &x in &TILE_AXIS {
            let offset = Vec2::new(x, z);
            if lod > 0 && is_inner(offset) {
                continue;
            }
            let (patch, rotation) = classify_patch(offset, lod == LOD_COUNT - 1);
            tiles.push(Tile {
                offset,
                patch,
                rotation,
            });
        }
    }
    tiles
}

fn add_quad(indices: &mut Vec<u32>, column: u32, row: u32, columns: u32) {
    let lower_left = column + row * columns;
    let lower_right = lower_left + 1;
    let upper_left = lower_left + columns;
    let upper_right = upper_left + 1;
    let quad = match (column + row).is_multiple_of(2) {
        true => [
            upper_right,
            lower_right,
            lower_left,
            lower_left,
            upper_left,
            upper_right,
        ],
        false => [
            upper_right,
            lower_right,
            upper_left,
            lower_left,
            upper_left,
            lower_right,
        ],
    };
    indices.extend_from_slice(&quad);
}

fn classify_patch(offset: Vec2, outer: bool) -> (Patch, f32) {
    let corner = is_corner(offset);
    if !corner && !is_border(offset) {
        return (Patch::Interior, 0.0);
    }
    let rotation = if corner {
        corner_rotation(offset)
    } else {
        side_rotation(offset)
    };
    if outer {
        let patch = match corner {
            true => Patch::FatXZOuter,
            false => Patch::FatXOuter,
        };
        return (patch, rotation);
    }
    if corner {
        return (corner_patch(offset), rotation);
    }

    let leading = match offset.x.abs() == OUTER_TILE_OFFSET {
        true => offset.x.is_sign_positive(),
        false => offset.y.is_sign_positive(),
    };
    let patch = if leading { Patch::SlimX } else { Patch::FatX };
    (patch, rotation)
}

fn corner_patch(offset: Vec2) -> Patch {
    match (offset.x.is_sign_positive(), offset.y.is_sign_positive()) {
        (false, true) => Patch::SlimXFatZ,
        (true, true) => Patch::SlimXZ,
        (false, false) => Patch::FatXZ,
        (true, false) => Patch::FatXSlimZ,
    }
}

fn side_rotation(offset: Vec2) -> f32 {
    match (
        offset.y.abs() >= offset.x.abs(),
        offset.x.is_sign_negative(),
    ) {
        (true, _) => -offset.y.signum() * std::f32::consts::FRAC_PI_2,
        (false, true) => std::f32::consts::PI,
        (false, false) => 0.0,
    }
}

fn corner_rotation(offset: Vec2) -> f32 {
    offset.x.atan2(offset.y) - std::f32::consts::FRAC_PI_4
}

fn patch_edges(patch: Patch) -> Edges {
    match patch {
        Patch::Interior => Edges::new(Edge::Regular, Edge::Regular),
        Patch::FatX => Edges::new(Edge::Fat, Edge::Regular),
        Patch::FatXSlimZ => Edges::new(Edge::Fat, Edge::Slim),
        Patch::FatXOuter => Edges::new(Edge::Fat, Edge::Regular).outer_x(),
        Patch::FatXZ => Edges::new(Edge::Fat, Edge::Fat),
        Patch::FatXZOuter => Edges::new(Edge::Fat, Edge::Fat).outer_xz(),
        Patch::SlimX => Edges::new(Edge::Slim, Edge::Regular),
        Patch::SlimXZ => Edges::new(Edge::Slim, Edge::Slim),
        Patch::SlimXFatZ => Edges::new(Edge::Slim, Edge::Fat),
    }
}

impl Edges {
    const fn new(x: Edge, z: Edge) -> Self {
        Self {
            x,
            z,
            outer_x: false,
            outer_z: false,
        }
    }

    const fn outer_x(mut self) -> Self {
        self.outer_x = true;
        self
    }

    const fn outer_xz(mut self) -> Self {
        self.outer_x = true;
        self.outer_z = true;
        self
    }
}

fn is_inner(offset: Vec2) -> bool {
    offset.x.abs() == INNER_TILE_OFFSET && offset.y.abs() == INNER_TILE_OFFSET
}

fn is_corner(offset: Vec2) -> bool {
    offset.x.abs() == OUTER_TILE_OFFSET && offset.y.abs() == OUTER_TILE_OFFSET
}

fn is_border(offset: Vec2) -> bool {
    offset.x.abs() == OUTER_TILE_OFFSET || offset.y.abs() == OUTER_TILE_OFFSET
}

#[cfg(test)]
#[path = "rings_tests.rs"]
mod tests;
