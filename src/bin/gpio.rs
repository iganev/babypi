use std::time::Duration;

use anyhow::anyhow;
use anyhow::Result;
use rppal::gpio::Gpio;
use rppal::uart::Parity;
use rppal::uart::Uart;

#[tokio::main]
async fn main() -> Result<()> {
    let gpio = Gpio::new().map_err(|e| anyhow!("Failed to init GPIO control: {}", e))?;

    let mmwave_gpio = gpio
        .get(18)
        .map_err(|e| anyhow!("Failed to bind to GPIO 18: {}", e))?
        .into_input_pulldown();

    println!("mmWave Radar presence: {}", mmwave_gpio.read());

    let mut ircut_ctrl_gpio = gpio
        .get(23)
        .map_err(|e| anyhow!("Failed to bind to GPIO 23: {}", e))?
        .into_output();

    println!("IRCut was {}", ircut_ctrl_gpio.is_set_high());

    ircut_ctrl_gpio.toggle();

    println!("IRCut is now {}", ircut_ctrl_gpio.is_set_high());

    let mut uart = Uart::new(115_200, Parity::None, 8, 1)
        .map_err(|e| anyhow!("Failed to init UART: {}", e))?;
    uart.set_read_mode(0, Duration::from_secs(5))?;

    let mut buf = [0; 255];

    loop {
        let len = uart
            .read(&mut buf)
            .map_err(|e| anyhow!("Failed to read UART: {}", e))?;

        if len > 0 {
            println!("UART: {}", String::from_utf8_lossy(&buf[0..len]));
        } else {
            println!("No data");
            break;
        }
    }

    Ok(())
}
