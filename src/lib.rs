use std::{path::PathBuf, task::Poll};

use aws_sdk_s3::types::ByteStream;
use aws_smithy_http::body::SdkBody;
use futures::{Stream, Future};
use hyper::body::Bytes;
use tokio::{fs::File, io::AsyncReadExt};

type CallbackFn = dyn Fn(u64, u64, u64) + Sync + Send + 'static;

pub struct TrackableBodyStream {
    tokio_file: File,
    file_size: u64,
    cur_read: u64,
    callback: Option<Box<CallbackFn>>,
    buffer_size: usize,
}

impl TryFrom<PathBuf> for TrackableBodyStream {
    type Error = std::io::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let file_size = std::fs::metadata(value.clone())?.len();
        let file = futures::executor::block_on(tokio::fs::File::open(value))?;
        Ok(Self {
            tokio_file: file, 
            file_size,
            cur_read: 0,
            callback: None,
            buffer_size: 2048,
        })
    }
}

impl TrackableBodyStream {
    pub fn with_callback(mut self, callback: impl Fn(u64, u64, u64) + Sync + Send + 'static) -> Self {
        self.callback = Some(Box::new(callback));
        self
    }

    pub fn set_callback(&mut self, callback: impl Fn(u64, u64, u64) + Sync + Send + 'static) {
        self.callback = Some(Box::new(callback));
    }

    pub fn set_buffer_size(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
    }

    pub fn content_length(&self) -> i64 {
        self.file_size as i64
    }

    pub fn to_s3_stream(self) -> ByteStream {
        let sdk_body = SdkBody::from(
            hyper::Body::from(
                Box::new(self) as Box<dyn Stream<Item = Result<hyper::body::Bytes, Box<(dyn std::error::Error + Sync + std::marker::Send + 'static)>>> + Send>
            )
        );
        ByteStream::new(sdk_body)
    }
}

impl Stream for TrackableBodyStream {
    type Item = Result<hyper::body::Bytes, Box<dyn std::error::Error + Sync + std::marker::Send + 'static>>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
        let mut_self = self.get_mut();
        let mut buf = vec!(0u8; mut_self.buffer_size);
        
        match Future::poll(Box::pin(mut_self.tokio_file.read_buf(&mut buf)).as_mut(), cx) {
            Poll::Ready(res) => {
                if res.is_err() {
                    return Poll::Ready(Some(Err(Box::new(res.err().unwrap()))));
                }
                let read_op = res.unwrap();
                if read_op == 0 {
                    return Poll::Ready(None);
                }
                mut_self.cur_read += read_op as u64;
                buf.resize(read_op, 0u8);
                if mut_self.callback.is_some() {
                    mut_self.callback.as_ref().unwrap()(mut_self.file_size, mut_self.cur_read, read_op as u64);
                }
                Poll::Ready(Some(Ok(Bytes::from(buf))))
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
