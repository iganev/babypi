use anyhow::Result;

use libpulse_binding as pulse;
use libpulse_simple_binding as simple;
use std::fs::File;
use std::io::Write;
use std::time::Instant;

pub fn main() -> Result<()> {
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

    let buffer_size = 14400; // 300ms
    let mut buffer = vec![0i16; buffer_size];

    let normalized_buffer_size = 28800;
    let mut normalized_buffer = vec![0f32; normalized_buffer_size];

    loop {
        let time = Instant::now();

        s.read(to_u8_slice(buffer.as_mut_slice()))?;

        normalized_buffer = buffer
            .iter()
            .map(|sample| *sample as f32 / 32768.0)
            .collect();

        let rms = calculate_rms(&normalized_buffer);

        println!("\rRMS: {rms:.3};\t\t{}", time.elapsed().as_millis());
    }

    // let mut file = File::create("test.raw")?;

    // let duration_secs = 60;
    // let iterations = (duration_secs * spec.rate as i32) / (buffer_size as i32 / 2);

    // for _ in 0..iterations {
    //     s.read(&mut buffer)?;
    //     file.write_all(&buffer)?;
    // }

    // println!("Recording finished. Saved to test.raw");

    // Ok(())
}

fn to_u8_slice(slice: &mut [i16]) -> &mut [u8] {
    let byte_len = 2 * slice.len();
    unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), byte_len) }
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    // Sum the squares of all samples
    let sum_of_squares: f32 = samples.iter().map(|sample| sample * sample).sum();

    // Calculate the mean of squares
    let mean_of_squares = sum_of_squares / samples.len() as f32;

    // Return the square root of the mean
    mean_of_squares.sqrt()
}
