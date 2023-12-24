use std::{mem, path::Path, slice, sync::Arc};

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use meshopt::VertexDataAdapter;
use metal::{Buffer, Device, MTLResourceOptions};

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub tex_coord: Vec2,
    pub normal: Vec3,
}

unsafe impl Zeroable for Vertex {}
unsafe impl Pod for Vertex {}

impl Vertex {
    #[inline]
    pub fn new(position: Vec3, tex_coord: Vec2, normal: Vec3) -> Self {
        Self {
            position,
            tex_coord,
            normal,
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct Meshlet {
    pub data_offset: u32,
    pub vertex_count: u32,
    pub triangle_count: u32,
}

unsafe impl Zeroable for Meshlet {}
unsafe impl Pod for Meshlet {}

impl Meshlet {
    #[inline]
    pub fn new(data_offset: u32, vertex_count: u32, triangle_count: u32) -> Self {
        Self {
            data_offset,
            vertex_count,
            triangle_count,
        }
    }
}

const MAX_VERTICES: usize = 64;
const MAX_TRIANGLES: usize = 124;
const CONE_WEIGHT: f32 = 0.0;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub meshlets: Vec<Meshlet>,
    pub meshlet_data: Vec<u32>,
}

impl Mesh {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mesh = fast_obj::Mesh::new(path)?;

        let mut vertices = vec![Default::default(); mesh.indices().len()];

        let positions = mesh.positions();
        let tex_coords = mesh.texcoords();
        let normals = mesh.normals();
        let indices = mesh.indices();

        for (i, index) in indices.iter().enumerate() {
            let position_idx = 3 * index.p as usize;
            let tex_coord_idx = 2 * index.t as usize;
            let normal_idx = 3 * index.n as usize;

            vertices[i] = Vertex::new(
                Vec3::new(
                    positions[position_idx],
                    positions[position_idx + 1],
                    positions[position_idx + 2],
                ),
                Vec2::new(tex_coords[tex_coord_idx], tex_coords[tex_coord_idx + 1]),
                Vec3::new(
                    normals[normal_idx],
                    normals[normal_idx + 1],
                    normals[normal_idx + 2],
                ),
            );
        }

        let (vertex_count, remap) = meshopt::generate_vertex_remap(&vertices, None);
        vertices.shrink_to(vertex_count);

        let mut vertices = meshopt::remap_vertex_buffer(&vertices, vertex_count, &remap);
        let mut indices = meshopt::remap_index_buffer(None, indices.len(), &remap);

        meshopt::optimize_vertex_cache_in_place(&mut indices, vertices.len());
        meshopt::optimize_overdraw_in_place(
            &mut indices,
            &VertexDataAdapter::new(bytemuck::cast_slice(&vertices), mem::size_of::<Vertex>(), 0)?,
            1.01,
        );
        meshopt::optimize_vertex_fetch_in_place(&mut indices, &mut vertices);

        let meshlets = meshopt::build_meshlets(
            &indices,
            &VertexDataAdapter::new(bytemuck::cast_slice(&vertices), mem::size_of::<Vertex>(), 0)?,
            MAX_VERTICES,
            MAX_TRIANGLES,
            CONE_WEIGHT,
        );

        let num_meshlet_data = meshlets
            .iter()
            .map(|meshlet| meshlet.vertices.len() + ((meshlet.triangles.len() * 3 + 3) >> 2))
            .sum();

        let mut meshlet_data = vec![0; num_meshlet_data];

        let mut index = 0;
        let meshlets = meshlets
            .iter()
            .map(|meshlet| {
                let data_offset = index;

                for vertex in meshlet.vertices {
                    meshlet_data[index] = *vertex;
                    index += 1;
                }

                let num_packed_indices = (meshlet.triangles.len() + 3) >> 2;
                for j in 0..num_packed_indices {
                    let triangle_offset = j << 2;
                    meshlet_data[index] = (meshlet.triangles[triangle_offset] as u32) << 0
                        | (meshlet
                            .triangles
                            .get(triangle_offset + 1)
                            .copied()
                            .unwrap_or_default() as u32)
                            << 8
                        | (meshlet
                            .triangles
                            .get(triangle_offset + 2)
                            .copied()
                            .unwrap_or_default() as u32)
                            << 16
                        | (meshlet
                            .triangles
                            .get(triangle_offset + 3)
                            .copied()
                            .unwrap_or_default() as u32) << 24;
                    index += 1;
                }

                Meshlet::new(
                    data_offset as _,
                    meshlet.vertices.len() as _,
                    (meshlet.triangles.len() / 3) as _,
                )
            })
            .collect();

        Ok(Self {
            vertices,
            meshlets,
            meshlet_data,
        })
    }
}

#[derive(Clone)]
pub struct MeshBuffers {
    pub vertex_buffer: Buffer,
    pub meshlet_buffer: Buffer,
    pub meshlet_data_buffer: Buffer,
    pub num_meshlets: usize,
}

impl MeshBuffers {
    pub unsafe fn new(device: &Device, path: impl AsRef<Path>) -> Result<Self> {
        let mesh = Mesh::new(path)?;

        let vertex_buffer = device.new_buffer_with_data(
            mesh.vertices.as_ptr() as *const _,
            (mesh.vertices.len() * mem::size_of::<Vertex>()) as _,
            MTLResourceOptions::StorageModeShared,
        );

        let meshlet_buffer = device.new_buffer_with_data(
            mesh.meshlets.as_ptr() as *const _,
            (mesh.meshlets.len() * mem::size_of::<Meshlet>()) as _,
            MTLResourceOptions::StorageModeShared,
        );

        let meshlet_data_buffer = device.new_buffer_with_data(
            mesh.meshlet_data.as_ptr() as *const _,
            (mesh.meshlet_data.len() * mem::size_of::<u32>()) as _,
            MTLResourceOptions::StorageModeShared,
        );

        Ok(Self {
            vertex_buffer,
            meshlet_buffer,
            meshlet_data_buffer,
            num_meshlets: mesh.meshlets.len(),
        })
    }
}
