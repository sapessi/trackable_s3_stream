# Trackable S3 stream
This library exposes an implementation of `hyper::Stream` that can be used as body for S3 requests with the AWS SDK for Rust. The stream object can send a callback with the current status of the stream.

Test the `indicaif` example by passing a bucket name

```bash
cargo run --example indicatif your_bucket_name
```