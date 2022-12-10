use std::{path::PathBuf, task::Poll};

use aws_sdk_s3::types::ByteStream;
use aws_smithy_http::body::SdkBody;
use futures::{Stream, Future};
use hyper::body::Bytes;
use tokio::{fs::File, io::AsyncReadExt};

const DEFAULT_BUFFER_SIZE: usize = 2048;

/// The callback function triggered every time a chunck of the source file is read
/// in the buffer.
/// 
/// # Arguments
/// * `u64`: The total length of the buffer (or size of the file if created from a `PathBuf`)
/// * `u64`: The total number of bytes read so far
/// * `u64`: The number of bytes read in the current chunck
type CallbackFn = dyn Fn(u64, u64, u64) + Sync + Send + 'static;

/// A `futures::Stream` implementation that can be used to track uploads to S3. As the S3 client
/// reads data from the stream it triggers a callback that can be used to update a UI.
/// 
/// A `TrackableBodyStream` can be constructed from a `PathBuf` with the `try_from` implementation
/// and from a `&[u8]`.
/// 
/// # Examples
/// ```
/// let mut body = TrackableBodyStream::try_from(PathBuf::from("./examples/sample.jpeg"))?;
/// let bar = ProgressBar::new(body.content_length() as u64);
///    
/// body.set_callback(move |tot_size: u64, sent: u64, cur_buf: u64| {
///    bar.inc(cur_buf as u64);
///    if sent == tot_size {
///        bar.finish();
///    }
///});
/// // this object can be passed to the put_object method of the AWS SDK for Rust.
/// let s3_stream = body.to_s3_stream();
/// ```
pub struct TrackableBodyStream<I: AsyncReadExt + Unpin> {
    input: I,
    file_size: u64,
    cur_read: u64,
    callback: Option<Box<CallbackFn>>,
    buffer_size: usize,
}

impl TryFrom<PathBuf> for TrackableBodyStream<File> {
    type Error = std::io::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let file_size = std::fs::metadata(value.clone())?.len();
        let file = futures::executor::block_on(tokio::fs::File::open(value))?;
        Ok(Self {
            input: file, 
            file_size,
            cur_read: 0,
            callback: None,
            buffer_size: DEFAULT_BUFFER_SIZE,
        })
    }
}

impl<'inputlife> From<&'inputlife [u8]> for TrackableBodyStream<&'inputlife [u8]> {
    fn from(value: &'inputlife [u8]) -> Self {
        let length = value.len();
        Self{
            input: value,
            file_size: length as u64,
            cur_read: 0,
            callback: None,
            buffer_size: DEFAULT_BUFFER_SIZE,
        }
    }
}

impl<I: AsyncReadExt + Unpin + Send + Sync + 'static> TrackableBodyStream<I> {
    /// Sets the callback method for the `TrackableBodyStream` and returns the populated
    /// stream.
    pub fn with_callback(mut self, callback: impl Fn(u64, u64, u64) + Sync + Send + 'static) -> Self {
        self.callback = Some(Box::new(callback));
        self
    }

    /// Sets the callback method
    pub fn set_callback(&mut self, callback: impl Fn(u64, u64, u64) + Sync + Send + 'static) {
        self.callback = Some(Box::new(callback));
    }

    /// Makes it easier to customize the size of the buffer used while reading from source
    pub fn set_buffer_size(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
    }

    /// This returns the size of the input file or slice. Can be used to set the `content_length`
    /// property of the `put_object` method in the AWS SDK for Rust to prevent S3 from closing the 
    /// connection for large objects without a known size
    pub fn content_length(&self) -> i64 {
        self.file_size as i64
    }

    /// Consumes this body stream and returns a `BodyStream` object that can be passed to the `body`
    /// method of the `put_object` call in the AWS SDK for Rust.
    pub fn to_s3_stream(self) -> ByteStream {
        let sdk_body = SdkBody::from(
            hyper::Body::from(
                Box::new(self) as Box<dyn Stream<Item = Result<hyper::body::Bytes, Box<(dyn std::error::Error + Sync + std::marker::Send + 'static)>>> + Send>
            )
        );
        ByteStream::new(sdk_body)
    }
}

impl<I: AsyncReadExt + Unpin> Stream for TrackableBodyStream<I> {
    type Item = Result<hyper::body::Bytes, Box<dyn std::error::Error + Sync + std::marker::Send + 'static>>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
        let mut_self = self.get_mut();
        let mut buf = Vec::with_capacity(mut_self.buffer_size);
        
        match Future::poll(Box::pin(mut_self.input.read_buf(&mut buf)).as_mut(), cx) {
            Poll::Ready(res) => {
                if res.is_err() {
                    return Poll::Ready(Some(Err(Box::new(res.err().unwrap()))));
                }
                let read_op = res.unwrap();
                if read_op == 0 {
                    return Poll::Ready(None);
                }
                mut_self.cur_read += read_op as u64;
                //buf.resize(read_op, 0u8);
                if mut_self.callback.is_some() {
                    mut_self.callback.as_ref().unwrap()(mut_self.file_size, mut_self.cur_read, read_op as u64);
                }
                Poll::Ready(Some(Ok(Bytes::from(Vec::from(&buf[0..read_op])))))
            },
            Poll::Pending => {
                Poll::Pending
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        ((self.file_size - self.cur_read) as usize, Some(self.file_size as usize))
    }
}
