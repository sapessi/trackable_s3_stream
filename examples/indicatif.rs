
use std::{path::PathBuf, env};
use aws_sdk_s3::Client;
use indicatif::ProgressBar;
use trackable_s3_stream::TrackableBodyStream;


#[tokio::main(flavor="current_thread")]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("The first argment must be the target S3 bucket name");
    }
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
    println!("Uploading to {}", bucket);
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
}