use std::fs::OpenOptions;
use std::io::Read;
use std::time::Instant;

use anyhow::Result;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ImageEncoder, RgbImage};
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use openh264::nal_units;

#[tokio::main]
async fn main() -> Result<()> {
    let mut time_start = Instant::now();

    println!("Loading input file...");
    let mut input = OpenOptions::new().read(true).open("test.h264")?;
    let mut buf = Vec::new();
    input.read_to_end(&mut buf)?;

    let mut decoder = Decoder::new()?;
    let mut img_data = Vec::new();
    let mut w: u32 = 0;
    let mut h: u32 = 0;

    println!("Took {} ms", time_start.elapsed().as_millis());
    time_start = Instant::now();

    println!("Looking for frames...");
    for packet in nal_units(&buf) {
        if let Ok(Some(frame)) = decoder.decode(packet) {
            println!("Encoding RGB data of packet with size {}...", packet.len());
            img_data = vec![0; frame.dimensions().0 * frame.dimensions().1 * 3];
            w = frame.dimensions().0 as u32;
            h = frame.dimensions().1 as u32;
            frame.write_rgb8(&mut img_data);
            break;
        }
    }

    println!("Took {} ms", time_start.elapsed().as_millis());
    time_start = Instant::now();

    if let Some(img) = RgbImage::from_raw(w, h, img_data) {
        println!("Opening file...");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open("frame.jpg")?;

        println!("Encoding JPEG image...");
        let encoder = JpegEncoder::new_with_quality(file, 80);

        match encoder.write_image(&img, w, h, image::ColorType::Rgb8) {
            // image::ExtendedColorType::Rgb8
            // encode_image(&img) {
            Ok(_) => {
                println!("Frame saved");
            }
            Err(e) => {
                println!("Failed to encode jpg image: {}", e);
            }
        }

        println!("Took {} ms", time_start.elapsed().as_millis());
        time_start = Instant::now();

        //

        println!("Opening file...");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open("frame.png")?;

        println!("Encoding PNG image...");
        let encoder = PngEncoder::new(file); //new_with_quality(file, 80);

        match encoder.write_image(&img, w, h, image::ColorType::Rgb8) {
            //image::ExtendedColorType::Rgb8
            Ok(_) => {
                println!("Frame saved");
            }
            Err(e) => {
                println!("Failed to encode png image: {}", e);
            }
        }

        println!("Took {} ms", time_start.elapsed().as_millis());
        time_start = Instant::now();

        //

        println!("Opening file...");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open("frame.webp")?;

        println!("Encoding WEBP image...");
        let encoder = WebPEncoder::new_lossless(file); //new_with_quality(file, 80);

        match encoder.write_image(&img, w, h, image::ColorType::Rgb8) {
            //image::ExtendedColorType::Rgb8
            Ok(_) => {
                println!("Frame saved");
            }
            Err(e) => {
                println!("Failed to encode webp image: {}", e);
            }
        }

        println!("Took {} ms", time_start.elapsed().as_millis());
    }

    Ok(())
}
