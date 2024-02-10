# Trackable S3 stream
This library exposes an implementation of `hyper::Stream` that can be used as body for S3 requests with the AWS SDK for Rust. The stream object can send a callback with the current status of the stream.

```rust
let mut body = TrackableBodyStream::try_from(PathBuf::from("./examples/sample.jpeg")).map_err(|e| {
    panic!("Could not open sample file: {}", e);
}).unwrap();

let bar = ProgressBar::new(body.content_length() as u64);
    
body.set_callback(move |tot_size: u64, sent: u64, cur_buf: u64| {
    bar.inc(cur_buf as u64);
    if sent == tot_size {
        bar.finish();
    }
});

let sdk_config = aws_config::from_env().load().await;
let s3_client = Client::new(&sdk_config);
let bucket = &args[1];
match s3_client.put_object()
    .bucket(bucket)
    .key("tracked_sample.jpeg")
    .content_length(body.content_length())
    .body(body.to_s3_stream())
    .send().await {
        Ok(_) => {
            println!("Upload complete");
        },
        Err(e) => {
            panic!("Failed to upload object: {}", e);
        }
    }
```

Test the `indicaif` example by passing a bucket name

```bash
cargo run --example indicatif your_bucket_name
```