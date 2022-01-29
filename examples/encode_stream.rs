use stream_encoder::{init_encoder, start_encoding, VideoSettings};

fn main() {
    init_encoder();

    let video_settings = VideoSettings::new(30, 300, 300);

    println!("Starting encoding");
    let (handle, image_sender) = start_encoding::<_, _, 3>("./test.mp4", video_settings);

    println!("Starting image sends");
    let images = std::fs::read_dir("./test_images").unwrap();
    let mut images = images.flatten().collect::<Vec<_>>();
    images.sort_by(|a, b| a.path().cmp(&b.path()));
    images.reverse();

    let images = images
        .into_iter()
        .map(|file| image::open(file.path()).unwrap().into_bgr8())
        .collect::<Vec<_>>();

    for _ in 0..10 {
        for image in &images {
            image_sender.send(image.clone()).unwrap();
        }
    }

    println!("Ending encoding stream");
    drop(image_sender);

    println!("Waiting for encoding thread to finish");
    handle.join().unwrap();
}
