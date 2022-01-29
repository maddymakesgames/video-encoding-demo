use std::sync::{Arc, Mutex, RwLock};

use stream_encoder::{data_provider::encode_video, init_encoder, VideoSettings};

fn main() {
    init_encoder();

    let video_settings = VideoSettings::new(30, 300, 300);

    let images = std::fs::read_dir("./test_images").unwrap();
    let mut images = images.flatten().collect::<Vec<_>>();
    images.sort_by(|a, b| a.path().cmp(&b.path()));
    images.reverse();

    let images = images
        .into_iter()
        .map(|file| image::open(file.path()).unwrap())
        .collect::<Vec<_>>();

    encode_video::<_, _, _, Option<()>>(
        "./owo.mp4".to_owned(),
        video_settings,
        stream_encoder::data_provider_impls::vec_data_provider,
        None,
        (Arc::new(Mutex::new(0)), Arc::new(RwLock::new(images))),
    );
}
