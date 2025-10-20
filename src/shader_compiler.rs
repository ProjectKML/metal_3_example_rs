use std::{
    ffi::{CStr, CString},
    fs, mem,
    ptr::NonNull,
};

use dispatch2::{dispatch_block_t, DispatchData};
use hassle_rs::compile_hlsl;
use metal_irconverter::{
    sys,
    sys::{
        IRObjectGetReflection, IRShaderReflectionCreate, IRShaderReflectionGetResourceCount,
        IRShaderReflectionGetResourceLocations, IRShaderStage,
        IRShaderStage_IRShaderStageAmplification, IRShaderStage_IRShaderStageCompute,
        IRShaderStage_IRShaderStageFragment, IRShaderStage_IRShaderStageMesh,
        IRShaderStage_IRShaderStageVertex,
    },
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSString;
use objc2_metal::{MTLBuffer, MTLDevice, MTLFunction, MTLLibrary, MTLSamplerState, MTLTexture};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShaderKind {
    Vertex,
    Fragment,
    Amplification,
    Mesh,
    Compute,
}

impl Into<&str> for ShaderKind {
    fn into(self) -> &'static str {
        match self {
            ShaderKind::Vertex => "vs_6_7",
            ShaderKind::Fragment => "ps_6_7",
            ShaderKind::Amplification => "as_6_7",
            ShaderKind::Mesh => "ms_6_7",
            ShaderKind::Compute => "cs_6_7",
        }
    }
}

impl ShaderKind {
    fn ir_shader_stage(self) -> IRShaderStage {
        match self {
            ShaderKind::Vertex => IRShaderStage_IRShaderStageVertex,
            ShaderKind::Fragment => IRShaderStage_IRShaderStageFragment,
            ShaderKind::Amplification => IRShaderStage_IRShaderStageAmplification,
            ShaderKind::Mesh => IRShaderStage_IRShaderStageMesh,
            ShaderKind::Compute => IRShaderStage_IRShaderStageCompute,
        }
    }
}

pub fn compile(
    device: &ProtocolObject<dyn MTLDevice>,
    path: &str,
    entry_point: &str,
    kind: ShaderKind,
) -> (
    Retained<ProtocolObject<dyn MTLLibrary>>,
    Retained<ProtocolObject<dyn MTLFunction>>,
) {
    let data = fs::read_to_string(path).unwrap();
    let dxil_code = compile_hlsl(path, &data, entry_point, kind.into(), &["-Zi"], &[]).unwrap();

    unsafe {
        let entry_point_cstr = CString::new(entry_point).unwrap();

        let compiler = sys::IRCompilerCreate();
        sys::IRCompilerSetEntryPointName(compiler, entry_point_cstr.as_ptr());

        let dxil = sys::IRObjectCreateFromDXIL(
            dxil_code.as_ptr(),
            dxil_code.len(),
            sys::IRBytecodeOwnership_IRBytecodeOwnershipNone,
        );
        let mut error = std::ptr::null_mut();
        let out_ir = sys::IRCompilerAllocCompileAndLink(
            compiler,
            entry_point_cstr.as_ptr(),
            dxil,
            &mut error,
        );

        if out_ir.is_null() {
            sys::IRErrorDestroy(error);
            panic!("IRCompilerAllocCompileAndLink failed");
        }

        let ir_shader_stage = kind.ir_shader_stage();

        let metal_lib = sys::IRMetalLibBinaryCreate();
        sys::IRObjectGetMetalLibBinary(out_ir, ir_shader_stage, metal_lib);
        let size = sys::IRMetalLibGetBytecodeSize(metal_lib);
        let mut bytecode = vec![0; size];
        sys::IRMetalLibGetBytecode(metal_lib, bytecode.as_mut_ptr());

        //Reflection
        let reflection = IRShaderReflectionCreate();
        if !IRObjectGetReflection(out_ir, ir_shader_stage, reflection) {
            panic!("IRObjectGetReflection failed");
        }

        let count = IRShaderReflectionGetResourceCount(reflection);
        let mut locations = Vec::with_capacity(count);
        IRShaderReflectionGetResourceLocations(reflection, locations.as_mut_ptr());
        locations.set_len(count);

        for location in locations {
            if !location.resourceName.is_null() {
                let name = CStr::from_ptr(location.resourceName).to_str().unwrap();
                println!("{}", name);
            }

            println!("{:?}", location);
            println!("-----------------------");
        }

        println!();

        sys::IRShaderReflectionDestroy(reflection);
        sys::IRMetalLibBinaryDestroy(metal_lib);
        sys::IRObjectDestroy(dxil);
        sys::IRObjectDestroy(out_ir);

        sys::IRCompilerDestroy(compiler);

        let library = device
            .newLibraryWithData_error(&DispatchData::new(
                NonNull::new(bytecode.as_ptr() as _).unwrap(),
                bytecode.len(),
                None,
                dispatch_block_t::default(),
            ))
            .unwrap();

        let function = library
            .newFunctionWithName(&NSString::from_str(entry_point))
            .unwrap();

        (library, function)
    }
}

#[cfg(test)]
mod tests {
    use objc2_metal::MTLCreateSystemDefaultDevice;

    use crate::shader_compiler::{compile, ShaderKind};

    #[test]
    fn compile_shader() {
        let device = MTLCreateSystemDefaultDevice().unwrap();

        let (_library, _mesh) = compile(
            &device,
            "shaders/geometry.hlsl",
            "geometry_mesh",
            ShaderKind::Mesh,
        );
        let (_library, _frag) = compile(
            &device,
            "shaders/geometry.hlsl",
            "geometry_pixel",
            ShaderKind::Fragment,
        );
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DescriptorTableEntry {
    gpu_va: u64,
    texture_view_id: u64,
    metadata: u64,
}

impl DescriptorTableEntry {
    pub fn texture(
        texture: &ProtocolObject<dyn MTLTexture>,
        min_lod_clamp: f32,
        metadata: u32,
    ) -> Self {
        unsafe {
            Self {
                gpu_va: 0,
                texture_view_id: mem::transmute(texture.gpuResourceID()),
                metadata: min_lod_clamp.to_bits() as u64 | ((metadata as u64) << 32),
            }
        }
    }

    pub fn sampler(sampler: &ProtocolObject<dyn MTLSamplerState>, lod_bias: f32) -> Self {
        unsafe {
            Self {
                gpu_va: mem::transmute(sampler.gpuResourceID()),
                texture_view_id: 0,
                metadata: lod_bias.to_bits() as u64,
            }
        }
    }

    pub fn buffer(buffer: &ProtocolObject<dyn MTLBuffer>, metadata: u64) -> Self {
        Self {
            gpu_va: buffer.gpuAddress(),
            texture_view_id: 0,
            metadata,
        }
    }
}
