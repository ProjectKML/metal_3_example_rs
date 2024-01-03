use glam::Mat4;
use metal::{Buffer, Device};
use crate::mesh::MeshBuffers;

#[repr(C)]
pub struct IndirectCommand {
    x: u32, y: u32, z: u32, id: u32,
    model_matrix: Mat4
}

pub struct IndirectCommandBuilder(Vec<IndirectCommand>);
impl IndirectCommandBuilder {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    
    pub fn add_command(&mut self, model_matrix: Mat4, mesh_buffers: &MeshBuffers) -> &mut Self {
        self.0.push(IndirectCommand {
            x: (mesh_buffers.num_meshlets * 32 + 31) as u32 / 32,
            y: 0,
            z: 0,
            id: 0,
            model_matrix,
        });

        self
    }

    pub fn build(device: &Device) -> Buffer {
        todo!()
    }
}

