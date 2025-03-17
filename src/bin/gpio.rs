use anyhow::anyhow;
use anyhow::Result;
use rppal::gpio::Gpio;

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

    Ok(())
}
