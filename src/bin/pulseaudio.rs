use anyhow::Result;

use libpulse_binding as pulse;
use libpulse_simple_binding as simple;
use std::fs::File;
use std::io::Write;

#[tokio::main]
pub async fn main() -> Result<()> {
    let spec = pulse::sample::Spec {
        format: pulse::sample::Format::S16le,
        channels: 1,
        rate: 48000,
    };

    let s = simple::Simple::new(
        None,
        "rust_recorder",
        pulse::stream::Direction::Record,
        Some("alsa_input.usb-DCMT_Technology_USB_Lavalier_Microphone_214b206000000178-00.mono-fallback"),
        "record",
        &spec,
        None,
        None,
    )?;

    let buffer_size = 1024 * 8;
    let mut buffer = vec![0u8; buffer_size];

    let mut file = File::create("test.raw")?;

    let duration_secs = 60;
    let iterations = (duration_secs * spec.rate as i32) / (buffer_size as i32 / 2);

    for _ in 0..iterations {
        s.read(&mut buffer)?;
        file.write_all(&buffer)?;
    }

    println!("Recording finished. Saved to test.raw");

    Ok(())
}
