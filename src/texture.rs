use std::{fs::File, io::BufReader, path::Path, ptr::NonNull};

use image::ImageFormat;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_metal::{
    MTLDevice, MTLOrigin, MTLPixelFormat, MTLRegion, MTLSize, MTLTexture, MTLTextureDescriptor,
};

pub struct ModelTexture {
    pub texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

impl ModelTexture {
    pub unsafe fn new(device: &ProtocolObject<dyn MTLDevice>, path: impl AsRef<Path>) -> Self {
        let file = File::open(path).unwrap();
        let image = image::load(BufReader::new(file), ImageFormat::Png).unwrap();

        let mut rgba8 = image.into_rgba8();

        let texture_descriptor = MTLTextureDescriptor::new();
        texture_descriptor.setWidth(rgba8.width() as _);
        texture_descriptor.setHeight(rgba8.height() as _);
        texture_descriptor.setPixelFormat(MTLPixelFormat::RGBA8Unorm);

        let texture = device
            .newTextureWithDescriptor(&texture_descriptor)
            .unwrap();

        texture.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: rgba8.width() as _,
                    height: rgba8.height() as _,
                    depth: 1,
                },
            },
            0,
            NonNull::new(rgba8.as_mut_ptr().cast()).unwrap(),
            (rgba8.width() * 4) as _,
        );

        Self { texture }
    }
}
