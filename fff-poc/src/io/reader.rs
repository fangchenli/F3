use bytes::Bytes;
use fff_core::errors::Result;
use futures::executor::block_on;
use lazy_static::lazy_static;
use object_store::path::Path;
use object_store::ObjectStore;
use parquet::file::reader::{ChunkReader, Length};
use std::io::Read;
use std::sync::{Arc, OnceLock};
use std::{fs::File, os::unix::fs::FileExt};
use tracing::{debug, error, instrument};

lazy_static! {
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Runtime::new()
        .expect("Failed to create tokio runtime for object store operations. This is a critical initialization failure.");
}

/// Read Trait for abstraction over local files and S3.
pub trait Reader {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<()>;
    fn size(&self) -> Result<u64>;
}

impl Reader for File {
    #[instrument(skip(self, buf), fields(size = buf.len(), offset))]
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        debug!("Reading from local file");
        let start = std::time::Instant::now();
        let result = FileExt::read_exact_at(self, buf, offset).map_err(Into::into);
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => debug!(elapsed_us = elapsed.as_micros(), "File read completed"),
            Err(e) => error!(error = %e, "File read failed"),
        }
        result
    }

    fn size(&self) -> Result<u64> {
        File::metadata(self).map(|m| m.len()).map_err(Into::into)
    }
}

impl Reader for Arc<File> {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        Reader::read_exact_at(self.as_ref(), buf, offset)
    }

    fn size(&self) -> Result<u64> {
        Reader::size(self.as_ref())
    }
}

impl Reader for [u8] {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        let offset = offset as usize;
        let end = offset
            .checked_add(buf.len())
            .ok_or_else(|| fff_core::errors::Error::General("Offset overflow".to_string()))?;

        if end > self.len() {
            return Err(fff_core::errors::Error::General(format!(
                "Read out of bounds: tried to read {} bytes at offset {} but slice length is {}",
                buf.len(),
                offset,
                self.len()
            )));
        }

        buf.copy_from_slice(&self[offset..end]);
        Ok(())
    }

    fn size(&self) -> Result<u64> {
        Ok(self.len() as u64)
    }
}

#[derive(Clone)]
pub struct ObjectStoreReadAt {
    object_store: Arc<dyn ObjectStore>,
    location: Arc<Path>,
    /// CAUTION: here we have the assumption that the file size won't change accross read requests.
    /// This is simply to allow Parquet readers to have less overhead on multiple reads.
    cache_size: OnceLock<u64>,
}

impl ObjectStoreReadAt {
    pub fn new(object_store: Arc<dyn ObjectStore>, location: Arc<Path>) -> Self {
        Self {
            object_store,
            location,
            cache_size: OnceLock::new(),
        }
    }
}

impl Reader for ObjectStoreReadAt {
    #[instrument(skip(self, buf), fields(size = buf.len(), offset, location = %self.location))]
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        let start = std::time::Instant::now();
        let start_range = offset as usize;

        debug!("Reading from object store");
        let object_store = Arc::clone(&self.object_store);
        let location = self.location.clone();
        let len = buf.len();
        let join_result = block_on(async move {
            RUNTIME
                .spawn(async move {
                    object_store
                        .get_range(&location, start_range..(start_range + len))
                        .await
                })
                .await
        });

        let head_result = join_result
            .map_err(|e| fff_core::errors::Error::General(format!("Task join error: {}", e)))?;
        let bytes = head_result.map_err(fff_core::errors::Error::ObjectStore)?;
        buf.copy_from_slice(bytes.as_ref());
        let elapsed = start.elapsed();
        debug!(
            elapsed_ms = elapsed.as_millis(),
            throughput_mbps = (len as f64 / elapsed.as_secs_f64() / 1_048_576.0),
            "Object store read completed"
        );
        Ok(())
    }

    #[instrument(skip(self), fields(location = %self.location))]
    fn size(&self) -> Result<u64> {
        // Try to get cached size, or compute it if not available
        if let Some(&size) = self.cache_size.get() {
            return Ok(size);
        }

        // Not cached, need to fetch it
        let start = std::time::Instant::now();
        debug!("Fetching object size from store");
        let object_store = Arc::clone(&self.object_store);
        let location = self.location.clone();
        let join_result = block_on(async move {
            RUNTIME
                .spawn(async move { object_store.head(&location).await })
                .await
        });

        let head_result = join_result
            .map_err(|e| fff_core::errors::Error::General(format!("Task join error: {}", e)))?;
        let elapsed = start.elapsed();
        let size = head_result
            .map_err(fff_core::errors::Error::ObjectStore)
            .map(|o| o.size as u64)?;
        debug!(
            size,
            elapsed_ms = elapsed.as_millis(),
            "Object size retrieved"
        );

        // Cache the result for future calls
        let _ = self.cache_size.set(size);
        Ok(size)
    }
}

impl Reader for Arc<ObjectStoreReadAt> {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        Reader::read_exact_at(self.as_ref(), buf, offset)
    }

    fn size(&self) -> Result<u64> {
        Reader::size(self.as_ref())
    }
}

impl Length for ObjectStoreReadAt {
    fn len(&self) -> u64 {
        self.size().unwrap()
    }
}

pub struct ObjectStoreRead {
    read_at: ObjectStoreReadAt,
    offset: usize,
}

impl Read for ObjectStoreRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read_at
            .read_exact_at(buf, self.offset as u64)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        self.offset += buf.len();
        Ok(buf.len())
    }
}

/// NOTE(Deprecated): This is no longer useful because it is sub-optimal to read Parquet.
/// We directly use async parquet reader now.
impl ChunkReader for ObjectStoreReadAt {
    type T = ObjectStoreRead;

    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        Ok(ObjectStoreRead {
            read_at: self.clone(),
            offset: start as usize,
        })
    }

    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        // let t = std::time::Instant::now();
        let start_range = start as usize;

        let object_store = Arc::clone(&self.object_store);
        let location = self.location.clone();
        let head_result = block_on(async move {
            RUNTIME
                .spawn(async move {
                    object_store
                        .get_range(&location, start_range..(start_range + length))
                        .await
                })
                .await
                .map_err(|e| object_store::Error::Generic {
                    store: "ObjectStoreReadAt",
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Task join error: {}", e),
                    )),
                })?
        });
        // println!("pq random access {:?}", t.elapsed());

        head_result.map_err(|err| parquet::errors::ParquetError::External(err.into()))
    }
}
