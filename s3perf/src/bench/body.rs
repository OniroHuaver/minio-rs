//! 将 generator [`Object`](crate::generator::Object) 转为 S3 [`ByteStream`](aws_sdk_s3::primitives::ByteStream)。

use crate::generator::Object;
use aws_sdk_s3::primitives::ByteStream;
use std::io::Read;

/// 同步读取对象全部字节（在 `spawn_blocking` 中调用以避免阻塞 runtime）。
pub fn read_object_bytes(mut obj: Object) -> std::io::Result<Vec<u8>> {
    if obj.size <= 0 {
        return Ok(Vec::new());
    }
    let cap = obj.size.min(i64::MAX) as usize;
    let mut buf = Vec::with_capacity(cap);
    obj.reader.read_to_end(&mut buf)?;
    Ok(buf)
}

/// 异步包装：大对象在阻塞线程池中读取。
pub async fn byte_stream_from_object(obj: Object) -> std::io::Result<ByteStream> {
    tokio::task::spawn_blocking(move || read_object_bytes(obj).map(ByteStream::from))
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
}
