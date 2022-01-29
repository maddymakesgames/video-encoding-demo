use gst_app::AppSrc;

use gst::{prelude::*, MessageView};
use gst_video::VideoInfo;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::{pipeline::init_pipeline, VideoSettings};

pub enum DataGenReturn {
    Result(anyhow::Result<()>),
    Option(Option<()>),
    Unit,
}

impl DataGenReturn {
    pub fn unwrap(self) {
        match self {
            DataGenReturn::Result(r) => r.unwrap(),
            DataGenReturn::Option(o) => o.unwrap(),
            DataGenReturn::Unit => {}
        }
    }
}

impl Into<anyhow::Result<()>> for DataGenReturn {
    fn into(self) -> anyhow::Result<()> {
        match self {
            DataGenReturn::Result(r) => r,
            DataGenReturn::Option(o) => o.ok_or(anyhow::Error::msg(
                "Data provider callback returned a None variant",
            )),
            DataGenReturn::Unit => Ok(()),
        }
    }
}

impl Into<DataGenReturn> for () {
    fn into(self) -> DataGenReturn {
        DataGenReturn::Unit
    }
}

impl Into<DataGenReturn> for anyhow::Result<()> {
    fn into(self) -> DataGenReturn {
        DataGenReturn::Result(self)
    }
}

impl Into<DataGenReturn> for Option<()> {
    fn into(self) -> DataGenReturn {
        DataGenReturn::Option(self)
    }
}

pub trait DataProvider<S: Send + Sync + Clone + 'static, O: Into<DataGenReturn> + 'static> {
    fn need_data(
        &self,
        appsrc: &AppSrc,
        video_info: &VideoInfo,
        video_settings: &VideoSettings,
        length: u32,
        state: S,
    ) -> O;
}

impl<
        S: Send + Sync + Clone + 'static,
        O: Into<DataGenReturn> + 'static,
        T: Fn(&AppSrc, &VideoInfo, &VideoSettings, u32, S) -> O,
    > DataProvider<S, O> for T
{
    fn need_data(
        &self,
        appsrc: &AppSrc,
        video_info: &VideoInfo,
        video_settings: &VideoSettings,
        length: u32,
        state: S,
    ) -> O {
        self(appsrc, video_info, video_settings, length, state)
    }
}

pub trait EnoughData<S: Send + Sync + Clone, O: Into<DataGenReturn> + 'static> {
    fn enough_data(&self, appsrc: &AppSrc, video_settings: &VideoSettings, state: S) -> O;
}

impl<
        S: Send + Sync + Clone,
        O: Into<DataGenReturn> + 'static,
        F: Fn(&AppSrc, &VideoSettings, S) -> O,
    > EnoughData<S, O> for F
{
    fn enough_data(&self, appsrc: &AppSrc, video_settings: &VideoSettings, state: S) -> O {
        self(appsrc, video_settings, state)
    }
}

impl<S: Send + Sync + Clone + 'static> EnoughData<S, ()> for Option<()> {
    fn enough_data(&self, _appsrc: &AppSrc, _video_settings: &VideoSettings, _state: S) -> () {}
}

impl<S: Send + Sync + Clone> EnoughData<S, anyhow::Result<()>> for Option<()> {
    fn enough_data(
        &self,
        _appsrc: &AppSrc,
        _video_settings: &VideoSettings,
        _state: S,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<S: Send + Sync + Clone + 'static> EnoughData<S, Option<()>> for Option<()> {
    fn enough_data(
        &self,
        _appsrc: &AppSrc,
        _video_settings: &VideoSettings,
        _state: S,
    ) -> Option<()> {
        Some(())
    }
}

pub fn encode_video<
    S: Send + Sync + Clone + 'static,
    O: Into<DataGenReturn> + 'static,
    P: DataProvider<S, O> + Send + Sync + 'static,
    E: EnoughData<S, O> + Send + Sync + 'static,
>(
    output_path: String,
    video_settings: VideoSettings,
    need_data: P,
    enough_data: Option<E>,
    state: S,
) {
    let (pipeline, appsrc, video_info) = init_pipeline(output_path, video_settings.clone());

    let state_clone = state.clone();

    let settings_clone = video_settings.clone();

    let mut builder = gst_app::AppSrcCallbacks::builder().need_data(move |appsrc, len| {
        let state = state.clone();
        need_data.need_data(appsrc, &video_info, &video_settings, len, state);
    });

    builder = if let Some(func) = enough_data {
        builder.enough_data(move |appsrc| {
            let state = state_clone.clone();
            func.enough_data(appsrc, &settings_clone, state);
        })
    } else {
        builder
    };

    appsrc.set_callbacks(builder.build());

    pipeline.set_state(gst::State::Playing).unwrap();

    let bus = pipeline.bus().unwrap();

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(_) => break,
            MessageView::Error(e) => {
                pipeline.set_state(gst::State::Null).unwrap();
                println!("Error! {e:?}");
            }
            MessageView::Progress(p) => println!("{p:?}"),
            MessageView::Warning(w) => println!("Warning: {w:?}"),
            MessageView::Info(i) => println!("Info: {i:?}"),
            _ => {}
        }
    }

    println!("ending pipeline");

    pipeline.set_state(gst::State::Null).unwrap();
}
