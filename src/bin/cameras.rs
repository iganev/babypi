use anyhow::Result;
use babypi::rpicam::Rpicam;

#[tokio::main]
async fn main() -> Result<()> {
    let res = Rpicam::list_cameras().await?;

    for (index, cam) in res.iter().enumerate() {
        println!("Camera {}: {:?}", index, cam);
    }

    Ok(())
}
