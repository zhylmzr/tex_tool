#![allow(clippy::enum_clike_unportable_variant)]
#![allow(dead_code)]
#[derive(Debug)]
pub enum TextureFormat {
    Dxt1 = 0,
    Dxt5 = 1,
    Rgb24 = 2,
    Argb32 = 3,
    R5g6b5 = 4,
    A4r4g4b4 = 5,
    Acf = 6,
    Unknow = 0xffffffff,
}

#[derive(Debug)]
#[repr(C)]
pub struct Header {
    pub size: u32,
    pub format: TextureFormat,
    pub data_size: u32,
    pub width: u32,
    pub height: u32,
    pub mipmap: u32,
    pub ex_frame_count: u16,
    pub frame_circle: u16,
    pub reserve: [u32; 9],
}

#[derive(Debug)]
#[repr(C)]
pub struct Texture {
    res: u32,
    version: u32,
    pub head: Header,
}

pub const TEXTURE_SIZE: usize = std::mem::size_of::<Texture>();
