use gst::{prelude::*, Caps, Pipeline};

use gst_app::AppSrc;

use gst_video::VideoInfo;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::VideoSettings;

pub fn init_encoder() {
    // This *seems* to not panic when called twice
    // So, um, it should be fine?
    // Should probably read more docs or smth
    gst::init().unwrap();
}

pub fn init_pipeline(
    output_path: String,
    video_settings: VideoSettings,
) -> (Pipeline, AppSrc, VideoInfo) {
    let pipeline = gst::Pipeline::new(Some("encoding pipeline"));

    let src = gst::ElementFactory::make("appsrc", Some("source")).unwrap();
    let videoconvert = gst::ElementFactory::make("videoconvert", Some("convert")).unwrap();
    let encoder = gst::ElementFactory::make(&video_settings.encoder, Some("encoder")).unwrap();
    let filter = gst::ElementFactory::make("capsfilter", None).unwrap();
    let muxer = gst::ElementFactory::make(&video_settings.muxer, Some("muxer")).unwrap();
    // let sink = gst::ElementFactory::make("filesink", Some("sink")).unwrap();
    let sink = gst::ElementFactory::make("filesink", Some("sink")).unwrap();

    sink.set_property("location", output_path);

    for (key, val) in video_settings.encoder_settings {
        encoder.set_property_from_str(&key, &val);
    }

    for (key, val) in video_settings.muxer_settings {
        muxer.set_property_from_str(&key, &val);
    }

    let output_info = Caps::builder("video/x-h264")
        .field("profile", "baseline")
        .field("speed-preset", "ultrafast")
        .build();

    filter.set_property("caps", &output_info);

    pipeline
        .add_many(&[&src, &videoconvert, &encoder, &filter, &muxer, &sink])
        .unwrap();
    gst::Element::link_many(&[&src, &videoconvert, &encoder, &filter, &muxer, &sink]).unwrap();

    let appsrc = src.dynamic_cast::<AppSrc>().unwrap();

    let video_info = gst_video::VideoInfo::builder(
        video_settings.format,
        video_settings.width,
        video_settings.height,
    )
    .fps(gst::Fraction::new(60, 1))
    .build()
    .unwrap();

    appsrc.set_caps(Some(&video_info.to_caps().unwrap()));
    appsrc.set_format(gst::Format::Time);

    (pipeline, appsrc, video_info)
}
