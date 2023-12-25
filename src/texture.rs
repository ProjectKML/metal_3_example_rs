use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use image::ImageFormat;
use metal::{Device, MTLOrigin, MTLPixelFormat, MTLRegion, MTLSize, Texture, TextureDescriptor};

pub struct ModelTexture {
    pub texture: Texture
}

impl ModelTexture {
    pub fn new(device: &Device, path: impl AsRef<Path>) -> Self {
        let file = File::open(path).unwrap();
        let image = image::load(BufReader::new(file),
            ImageFormat::Png)
            .unwrap();

        let rgba8 = image.into_rgba8();

        let mut texture_descriptor = TextureDescriptor::new();
        texture_descriptor.set_width(rgba8.width() as _);
        texture_descriptor.set_height(rgba8.height() as _);
        texture_descriptor.set_pixel_format(MTLPixelFormat::RGBA8Unorm);

        let texture = device.new_texture(&texture_descriptor);

        texture.replace_region(
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: rgba8.width() as _,
                    height: rgba8.height() as _,
                    depth: 1,
                },
            },
            0,
            rgba8.as_ptr() as *const _,
            (rgba8.width() * 4) as _,
        );

        Self {
            texture
        }
    }
}