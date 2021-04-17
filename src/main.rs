mod texture;

use futures::{stream, Stream, StreamExt};
use image::{ImageBuffer, Rgba, RgbaImage};
use squish::Format::{Bc1, Bc3};
use std::{collections::VecDeque, convert::TryInto, io::Result, mem::transmute, path::PathBuf};
use tokio::{
    fs::{read_dir, DirEntry, File},
    io::AsyncReadExt,
};

use texture::*;

fn parse_tex(buf: &[u8]) -> Texture {
    assert!(buf.len() == TEXTURE_SIZE);

    unsafe { transmute::<[u8; TEXTURE_SIZE], Texture>(buf.try_into().unwrap()) }
}

fn visit(path: impl Into<PathBuf>) -> impl Stream<Item = Result<DirEntry>> + Send + 'static {
    async fn one_level(path: PathBuf, to_visit: &mut Vec<PathBuf>) -> Result<Vec<DirEntry>> {
        let mut dir = read_dir(path).await?;
        let mut files = Vec::new();

        while let Some(child) = dir.next_entry().await? {
            if child.metadata().await?.is_dir() {
                to_visit.push(child.path());
            } else if child.file_name().into_string().unwrap().ends_with(".tex") {
                files.push(child)
            }
        }
        Ok(files)
    }

    stream::unfold(vec![path.into()], |mut to_visit| async {
        let path = to_visit.pop()?;
        let file_stream = match one_level(path, &mut to_visit).await {
            Ok(files) => stream::iter(files).map(Ok).left_stream(),
            Err(e) => stream::once(async { Err(e) }).right_stream(),
        };
        Some((file_stream, to_visit))
    })
    .flatten()
}

async fn _get_format(path: PathBuf) -> u16 {
    let mut file = File::open(&path).await.unwrap();
    let mut tex = [0u8; TEXTURE_SIZE];
    file.read_exact(&mut tex).await.unwrap();
    let tex = parse_tex(&tex);

    if tex.head.ex_frame_count > 0 {
        println!("{:?}", path);
    }
    tex.head.ex_frame_count
}

async fn read_u32(file: &mut File) -> u32 {
    let mut buf = vec![0u8; 4];
    file.read_exact(&mut buf).await.unwrap();
    unsafe { transmute::<[u8; 4], u32>(buf.try_into().unwrap()) }
}

async fn save_image(path: PathBuf, output_dir: &str) {
    let mut file = File::open(&path).await.unwrap();
    let mut tex = [0u8; TEXTURE_SIZE];
    file.read_exact(&mut tex).await.unwrap();
    let tex = parse_tex(&tex);

    let size = read_u32(&mut file).await;

    if size != tex.head.data_size {
        println!("数据长度不一致 {:?}", path);
        return;
    }

    let mut buf = vec![0u8; size as usize];
    file.read_exact(&mut buf)
        .await
        .unwrap_or_else(|_| panic!("error read length in {:?}", path));

    // rgba
    let colors = match tex.head.format {
        TextureFormat::Dxt1 => save_dxt1(&buf, tex.head.width, tex.head.height),
        TextureFormat::Dxt5 => save_dxt5(&buf, tex.head.width, tex.head.height),
        TextureFormat::Rgb24 => save_rgb24(&buf),
        TextureFormat::Argb32 => save_argb32(&buf),
        TextureFormat::R5g6b5 => save_r5g6b5(&buf),
        TextureFormat::A4r4g4b4 => save_a4r4g4b4(&buf),
        format => {
            println!("暂不支持的格式 {:?} {:?}", format, path);
            vec![]
        }
    };
    if colors.is_empty() {
        return;
    }

    let mut colors = colors.iter().collect::<VecDeque<_>>();

    let mut image: RgbaImage = ImageBuffer::new(tex.head.width, tex.head.height);
    for pixiel in image.pixels_mut() {
        let color = colors.pop_front().unwrap();
        *pixiel = Rgba([color[0], color[1], color[2], color[3]]);
    }

    let filename = path.file_name().unwrap().to_str().unwrap();
    image
        .save(format!(
            "{}/{}",
            output_dir,
            filename.replace(".tex", ".png"),
        ))
        .unwrap();
}

fn save_a4r4g4b4(buf: &[u8]) -> Vec<[u8; 4]> {
    let mut colors = vec![];
    buf.chunks(2).for_each(|va| unsafe {
        let color = transmute::<[u8; 2], u16>(va.try_into().unwrap());
        let a = (color & 0xf000) >> 12;
        let r = (color & 0xf00) >> 8;
        let g = (color & 0xf0) >> 4;
        let b = color & 0xf;

        let a = (a as f32) * 255.0 / 15.0;
        let r = (r as f32) * 255.0 / 15.0;
        let g = (g as f32) * 255.0 / 15.0;
        let b = (b as f32) * 255.0 / 15.0;

        colors.push([r as u8, g as u8, b as u8, a as u8]);
    });
    colors
}

fn save_r5g6b5(buf: &[u8]) -> Vec<[u8; 4]> {
    let mut colors = vec![];
    buf.chunks(2).for_each(|va| unsafe {
        let color = transmute::<[u8; 2], u16>(va.try_into().unwrap());
        let r = (color & 0xf800) >> 11;
        let g = (color & 0x7e00) >> 5;
        let b = color & 0x1f;

        let r = (r as f32) * 255.0 / 31.0;
        let g = (g as f32) * 255.0 / 63.0;
        let b = (b as f32) * 255.0 / 31.0;

        colors.push([r as u8, g as u8, b as u8, 255]);
    });
    colors
}

fn save_rgb24(buf: &[u8]) -> Vec<[u8; 4]> {
    let mut colors = vec![];
    buf.chunks(3).for_each(|va| {
        let mut va = Vec::from(va);
        va.push(255);
        colors.push(va[0..4].try_into().unwrap());
    });
    colors
}

// rgba
fn save_argb32(buf: &[u8]) -> Vec<[u8; 4]> {
    let mut colors = vec![];
    buf.chunks(4).for_each(|va| {
        // colors.push([va[1], va[2], va[3], va[0]]);
        // I also don't know why the order
        colors.push([va[2], va[1], va[0], va[3]]);
    });
    colors
}

fn save_dxt1(buf: &[u8], width: u32, height: u32) -> Vec<[u8; 4]> {
    // 8:1
    let mut output = vec![0u8; buf.len() * 8];
    Bc1.decompress(buf, width as usize, height as usize, &mut output);

    let mut colors = vec![];
    output.chunks(4).for_each(|c| {
        colors.push(c.try_into().unwrap());
    });

    colors
}

fn save_dxt5(buf: &[u8], width: u32, height: u32) -> Vec<[u8; 4]> {
    // 4:1
    let mut output = vec![0u8; buf.len() * 8];
    Bc3.decompress(buf, width as usize, height as usize, &mut output);

    let mut colors = vec![];
    output.chunks(4).for_each(|c| {
        colors.push(c.try_into().unwrap());
    });
    //argb
    colors
}

async fn bootstrap(path: &str, output: &str) {
    let paths = visit(path);
    let out_dir = PathBuf::from(output);
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir).unwrap();
    }

    paths
        .for_each(|entry| async {
            match entry {
                Ok(entry) => {
                    save_image(entry.path(), output).await;
                }
                Err(e) => eprintln!("encountered an error: {}", e),
            }
        })
        .await;
}

use clap::{App, Arg};

#[tokio::main]
async fn main() {
    let matches = App::new("X-Project texture tool")
        .version("0.0.1")
        .author("zhylmzr <zhylmzr@gmail.com>")
        .arg(
            Arg::with_name("input")
                .help("Set the directory of textures")
                .required(false)
                .index(1),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("dir")
                .help("Set the output directory")
                .required(false),
        )
        .get_matches();

    let input = matches.value_of("input").unwrap();
    let output = matches.value_of("output").unwrap_or("output");

    bootstrap(input, output).await;
}
