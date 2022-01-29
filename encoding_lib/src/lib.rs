#![doc = include_str!("../README.md")]
use ::gstreamer::Caps;
use gstreamer_video::VideoFormat;
use image::{DynamicImage, ImageBuffer, Pixel};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};

use std::{
    sync::mpsc::{channel, Receiver, Sender},
    thread::JoinHandle,
};

use crate::data_provider::encode_video;
pub use crate::pipeline::init_encoder;

/// Re-exports from the gstreamer crates to allow extra customization
pub mod gstreamer {
    pub use gstreamer::*;
    pub mod video {
        pub use gstreamer_video::*;
    }

    pub mod app {
        pub use gstreamer_app::*;
    }
}

pub mod data_provider;
pub mod data_provider_impls;
pub mod pipeline;

/// The different settings you can set for the encoder
#[derive(Debug, Clone)]
pub struct VideoSettings {
    /// The framerate of the video
    pub framerate: u64,
    /// The width of the video
    pub width: u32,
    /// The height of the video
    pub height: u32,
    /// The encoder plugin to use
    pub encoder: String,
    /// The muxer plugin to use
    pub muxer: String,
    /// The format of images sent into the app pipeline
    pub format: VideoFormat,
    /// Restrictions on video format to put on the encoder
    pub caps: Caps,
    pub encoder_settings: HashMap<String, String>,
    pub muxer_settings: HashMap<String, String>,
}

impl VideoSettings {
    pub fn new(framerate: u64, width: u32, height: u32) -> Self {
        VideoSettings {
            framerate: framerate,
            width,
            height,
            encoder: "x264enc".to_owned(),
            muxer: "mp4mux".to_owned(),
            format: VideoFormat::Bgrx,
            // TODO: somehow make this support any video encoding? idk how I would do that
            // it would be nice to change the video encoding without *having* to change the caps
            // though typically you would have to anyway
            caps: Caps::builder("video/x-h264").build(),
            encoder_settings: HashMap::new(),
            muxer_settings: HashMap::new(),
        }
    }
}

/// Spawns a thread to do encoding, returning a channel to send frame data through.
///
/// It is safe to detach the thread as it will automatically close when the encoding is finished.
///
/// The `BUFFER_SIZE` associated constant is how many frames the encoder
/// will wait for before continuing the encoding.<br>
/// If the sender is dropped and `BUFFER_SIZE` is not able to be met
/// the encoder will exit properly and encode however many frames it was able to get.
///
/// # Deadlock
/// Joining the thread before dropping the sender will deadlock.
pub fn start_encoding<
    Format: Pixel<Subpixel = u8> + Send + Sync + 'static,
    Container: Deref<Target = [Format::Subpixel]> + Send + Sync + 'static,
    const BUFFER_SIZE: usize,
>(
    output_path: &str,
    video_settings: VideoSettings,
) -> (JoinHandle<()>, Sender<ImageBuffer<Format, Container>>) {
    let (sender, recv) = channel();

    let path = output_path.to_owned();

    let handle = std::thread::spawn(|| {
        start_encoding_internal::<Format, Container, BUFFER_SIZE>(recv, path, video_settings)
    });

    (handle, sender)
}

fn start_encoding_internal<
    Format: Pixel<Subpixel = u8> + Send + Sync + 'static,
    Container: Deref<Target = [Format::Subpixel]> + Send + Sync + 'static,
    const BUFFER_SIZE: usize,
>(
    recv: Receiver<ImageBuffer<Format, Container>>,
    output_path: String,
    video_settings: VideoSettings,
) {
    init_encoder();

    encode_video::<_, _, _, Option<()>>(
        output_path,
        video_settings,
        data_provider_impls::reciever_data_provider::<Format, Container, BUFFER_SIZE>,
        None,
        (Arc::new(Mutex::new(0)), Arc::new(Mutex::new(recv))),
    );
}

/// Encodes a set of frames
///
/// Blocks the current thread till the encoding is done
pub fn encode_frames(output_path: &str, video_settings: VideoSettings, frames: Vec<DynamicImage>) {
    init_encoder();
    encode_video::<_, _, _, Option<()>>(
        output_path.to_owned(),
        video_settings,
        data_provider_impls::vec_data_provider,
        None,
        (Arc::new(Mutex::new(0)), Arc::new(RwLock::new(frames))),
    );
}
