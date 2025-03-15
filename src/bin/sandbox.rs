use babypi::rpicam::Rpicam;

#[tokio::main]
async fn main() {
    let res = Rpicam::list_cameras().await;
}
