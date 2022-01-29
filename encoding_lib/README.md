# Stream Encoder

This is a library designed to make encoding videos from a stream of frames easy.

This is based off of [gstreamer](https://gstreamer.freedesktop.org/) using the [gstreamer-rs bindings](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs)

To get started with this library you can use `start_encoding`
```rust
use image;
use stream_encoder::start_encoding;

// Start the encoding thread
let (encoding_thread, frame_sender) = start_encoding::<3>();

// load in the frame
let frame = image::open("./test.png").unwrap();

// send 10 copies of the frame
for _ in 0..10 {
    frame_sender.send(frame.clone()).unwrap();
}

// end encoding
drop(frame_sender);

// wait for the encoder to finalize
encoding_thread.join().unwrap();
```

If you need more control over how data is sent to the encoder, you can make your own data provider.
