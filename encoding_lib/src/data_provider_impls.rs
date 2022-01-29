use std::{
    ops::Deref,
    sync::{mpsc::Receiver, Arc, Mutex, RwLock},
};

use gst_app::AppSrc;

use gst_video::VideoInfo;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use image::{DynamicImage, ImageBuffer, Pixel};

use crate::VideoSettings;

pub fn reciever_data_provider<
    Format: Pixel<Subpixel = u8> + 'static,
    Container: Deref<Target = [Format::Subpixel]>,
    const BUFFER_SIZE: usize,
>(
    appsrc: &AppSrc,
    video_info: &VideoInfo,
    video_settings: &VideoSettings,
    _length: u32,
    state: (
        Arc<Mutex<u64>>,
        Arc<Mutex<Receiver<ImageBuffer<Format, Container>>>>,
    ),
) {
    let mut frame_num = state.0.lock().unwrap();
    let receiver = state.1.lock().unwrap();
    println!("frames requested, currently provided {frame_num} frames of video");

    for _ in 0..BUFFER_SIZE {
        let mut buffer = gst::Buffer::with_size(video_info.size()).unwrap();
        if let Ok(image) = receiver.recv() {
            let buffer = buffer.get_mut().unwrap();

            buffer
                .set_pts(*frame_num * (1_000 / video_settings.framerate) * gst::ClockTime::MSECOND);

            let mut pixels = image.pixels().map(|p| p.to_bgra());

            let mut vframe =
                gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, &video_info).unwrap();

            let width = vframe.width() as usize;
            let height = vframe.height() as usize;
            let stride = vframe.plane_stride()[0] as usize;

            for line in vframe
                .plane_data_mut(0)
                .unwrap()
                .chunks_exact_mut(stride)
                .take(height)
            {
                for pixel in line[..(4 * width)].chunks_exact_mut(4) {
                    if let Some(frame_pixels) = pixels.next() {
                        pixel[0] = frame_pixels[0];
                        pixel[1] = frame_pixels[1];
                        pixel[2] = frame_pixels[2];
                        pixel[3] = frame_pixels[3];
                    }
                }
            }
            *frame_num += 1;
        } else {
            println!("End of video stream detected!");
            let _ = appsrc.end_of_stream();
            return;
        }

        let _ = appsrc.push_buffer(buffer).unwrap();
    }
}

pub fn vec_data_provider(
    appsrc: &AppSrc,
    video_info: &VideoInfo,
    video_settings: &VideoSettings,
    _length: u32,
    state: (Arc<Mutex<u64>>, Arc<RwLock<Vec<DynamicImage>>>),
) {
    let mut frame_num = state.0.lock().unwrap();
    let images = state.1.read().unwrap();

    if *frame_num as usize == images.len() {
        let _ = appsrc.end_of_stream().unwrap();
        return;
    }

    let mut buffer = gst::Buffer::with_size(video_info.size()).unwrap();

    {
        let image = images.get(*frame_num as usize).unwrap();
        let buffer = buffer.get_mut().unwrap();

        buffer.set_pts(
            *frame_num * (1000 / video_settings.framerate) as u64 * gst::ClockTime::MSECOND,
        );

        // Expensive clone, try to remove
        let image_rgb = image.clone().into_rgba8();
        let mut pixels = image_rgb.pixels().map(|p| p.0);

        let mut vframe =
            gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, &video_info).unwrap();

        let width = vframe.width() as usize;
        let height = vframe.height() as usize;
        let stride = vframe.plane_stride()[0] as usize;

        for line in vframe
            .plane_data_mut(0)
            .unwrap()
            .chunks_exact_mut(stride)
            .take(height)
        {
            for pixel in line[..(4 * width)].chunks_exact_mut(4) {
                if let Some(frame_pixels) = pixels.next() {
                    pixel[0] = frame_pixels[0];
                    pixel[1] = frame_pixels[1];
                    pixel[2] = frame_pixels[2];
                    pixel[3] = 0;
                }
            }
        }

        *frame_num += 1;
    }

    let _ = appsrc.push_buffer(buffer).unwrap();
}
