use std::path::Path;

pub mod config;
pub mod ffmpeg;
pub mod gpio;
pub mod live_stream;
pub mod mlx90640;
pub mod mmwave;
pub mod process_control;
pub mod rpicam;

/// Check if file exists
pub async fn file_exists(file: impl AsRef<Path>) -> bool {
    tokio::fs::try_exists(file).await.is_ok_and(|res| res)
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }
