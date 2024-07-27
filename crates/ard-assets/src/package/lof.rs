use std::{
    fs::OpenOptions,
    io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write},
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use camino::Utf8PathBuf;
use crossbeam_utils::sync::{ShardedLock, ShardedLockReadGuard, ShardedLockWriteGuard};
use path_slash::PathBufExt;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::prelude::{AssetName, AssetNameBuf};

use super::{
    manifest::{FileMetaData, Manifest},
    PackageInterface, PackageOpenError, PackageReadError,
};

pub const VERSION: u32 = 0;
pub const COMPRESSION_LEVEL: i32 = 9;

#[derive(Clone)]
pub struct LofPackage(Arc<LofPackageInner>);

struct LofPackageInner {
    path: PathBuf,
    lof_manifest: LofManifest,
    ard_manifest: ShardedLock<Manifest>,
    file: tokio::sync::Mutex<tokio::io::BufReader<tokio::fs::File>>,
}

impl LofPackage {
    pub async fn open(path: &Path) -> Result<Self, PackageOpenError> {
        if !path.exists() || !path.is_file() {
            return Err(PackageOpenError::DoesNotExist);
        }

        // Read in the manifest
        let file = OpenOptions::new().read(true).open(path).unwrap();
        let mut reader = BufReader::new(file);

        let mut version = u32::MAX;
        let mut manifest_base = u64::MAX;
        let mut manifest_size = 0_u64;

        reader
            .read_exact(bytemuck::bytes_of_mut(&mut version))
            .unwrap();
        reader
            .read_exact(bytemuck::bytes_of_mut(&mut manifest_base))
            .unwrap();
        reader
            .read_exact(bytemuck::bytes_of_mut(&mut manifest_size))
            .unwrap();

        reader.seek(SeekFrom::Start(manifest_base)).unwrap();
        let lof_manifest = zstd::decode_all(reader).unwrap();
        let lof_manifest = bincode::deserialize::<LofManifest>(&lof_manifest).unwrap();

        // Convert it into an ard manifest
        let mut ard_manifest = Manifest::default();
        ard_manifest.assets = lof_manifest
            .assets
            .iter()
            .map(|(p, _)| (p.clone(), FileMetaData::default()))
            .collect();

        // Open the file for tokio
        let file = tokio::fs::File::open(path).await.unwrap();
        let file = tokio::io::BufReader::new(file);

        Ok(Self(Arc::new(LofPackageInner {
            path: path.into(),
            lof_manifest,
            ard_manifest: ShardedLock::new(ard_manifest),
            file: tokio::sync::Mutex::new(file),
        })))
    }
}

#[async_trait]
impl PackageInterface for LofPackage {
    #[inline]
    fn path(&self) -> &Path {
        &self.0.path
    }

    #[inline]
    fn manifest(&self) -> ShardedLockReadGuard<Manifest> {
        self.0.ard_manifest.read().unwrap()
    }

    #[inline]
    fn manifest_mut(&self) -> ShardedLockWriteGuard<Manifest> {
        unimplemented!("lof packages are immutable")
    }

    fn register_asset(&self, name: &AssetName) -> bool {
        self.0.lof_manifest.assets.contains_key(name)
    }

    async fn read(&self, file: Utf8PathBuf) -> Result<Vec<u8>, PackageReadError> {
        // I would prefer not to have to heap allocate. Look into replacing
        let file: Utf8PathBuf = file
            .into_std_path_buf()
            .to_slash()
            .unwrap()
            .to_string()
            .into();
        let (offset, size) = match self.0.lof_manifest.assets.get(&file) {
            Some(props) => props,
            None => return Err(PackageReadError::DoesNotExist(file.to_owned())),
        };

        let mut contents = Vec::with_capacity(*size as usize);
        let mut file = self.0.file.lock().await;
        file.seek(SeekFrom::Start(*offset)).await.unwrap();
        let decoder =
            async_compression::futures::bufread::ZstdDecoder::new(file.deref_mut().compat());
        decoder.compat().read_to_end(&mut contents).await?;

        Ok(contents)
    }

    async fn read_str(&self, file: Utf8PathBuf) -> Result<String, PackageReadError> {
        let contents = self.read(file).await?;
        let contents = async { String::from_utf8(contents).unwrap() }.await;
        Ok(contents)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct LofManifest {
    assets: FxHashMap<AssetNameBuf, (u64, u64)>,
}

pub fn create_lof_from_folder(out: impl Into<PathBuf>, src: impl Into<PathBuf>) {
    let out: PathBuf = out.into();
    let src: PathBuf = src.into();

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(out)
        .unwrap();
    let mut writer = BufWriter::new(file);

    // Write version and default header offset and size
    writer.write_all(bytemuck::bytes_of(&VERSION)).unwrap();
    writer.write_all(bytemuck::bytes_of(&0_u64)).unwrap();
    writer.write_all(bytemuck::bytes_of(&0_u64)).unwrap();

    let mut manifest = LofManifest::default();

    // Compress all files in the source
    for entry in src.read_dir().unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.metadata().unwrap();

        if metadata.is_dir() | metadata.is_symlink() {
            continue;
        }

        let asset_file = OpenOptions::new().read(true).open(entry.path()).unwrap();
        let reader = BufReader::new(asset_file);
        let compressed = zstd::encode_all(reader, COMPRESSION_LEVEL).unwrap();

        let asset_name = entry.file_name();
        let base = writer.stream_position().unwrap();
        let size = compressed.len() as u64;
        manifest.assets.insert(
            Utf8PathBuf::from_path_buf(asset_name.into()).unwrap(),
            (base, size),
        );

        writer.write_all(&compressed).unwrap();
    }

    // Serialize and compress the manifest
    let manifest = bincode::serialize(&manifest).unwrap();
    let manifest = zstd::encode_all(Cursor::new(manifest), COMPRESSION_LEVEL).unwrap();

    let base = writer.stream_position().unwrap();
    let size = manifest.len() as u64;

    writer.write_all(&manifest).unwrap();

    // Update manifest base and size in file
    writer
        .seek(SeekFrom::Start(std::mem::size_of_val(&VERSION) as u64))
        .unwrap();
    writer.write_all(bytemuck::bytes_of(&base)).unwrap();
    writer.write_all(bytemuck::bytes_of(&size)).unwrap();
}

pub fn create_lof_from_folder_mt(
    out: impl Into<PathBuf>,
    src: impl Into<PathBuf>,
    thread_count: usize,
) {
    let out: PathBuf = out.into();
    let src: PathBuf = src.into();

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(out)
        .unwrap();
    let mut writer = BufWriter::new(file);

    // Write version and default header offset and size
    writer.write_all(bytemuck::bytes_of(&VERSION)).unwrap();
    writer.write_all(bytemuck::bytes_of(&0_u64)).unwrap();
    writer.write_all(bytemuck::bytes_of(&0_u64)).unwrap();

    let mut manifest = LofManifest::default();

    struct DstFile {
        name: AssetNameBuf,
        data: Vec<u8>,
        base: u64,
        size: u64,
    }

    let (src_file_send, src_file_recv) = crossbeam_channel::unbounded::<AssetNameBuf>();
    let (dst_file_send, dst_file_recv) = crossbeam_channel::unbounded::<DstFile>();
    let base_ptr = Arc::new(AtomicU64::new(writer.stream_position().unwrap()));

    // Append all files that must be sent
    for entry in src.read_dir().unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.metadata().unwrap();

        if metadata.is_dir() | metadata.is_symlink() {
            continue;
        }

        src_file_send
            .send(Utf8PathBuf::from_path_buf(entry.path()).unwrap())
            .unwrap();
    }

    // Spin up thread to write files to disk and create the manifest
    let file_count = src_file_send.len();
    let manifest_builder = std::thread::spawn(move || {
        let mut count = 0;
        while count < file_count {
            let file = dst_file_recv.recv().unwrap();
            manifest.assets.insert(file.name, (file.base, file.size));
            writer.seek(SeekFrom::Start(file.base)).unwrap();
            writer.write_all(&file.data).unwrap();
            count += 1;
        }

        (writer, manifest)
    });

    // Spin up threads to compress files
    let writer_threads = (0..thread_count)
        .into_iter()
        .map(|_| {
            let base_ptr = base_ptr.clone();
            let src_file_recv = src_file_recv.clone();
            let dst_file_send = dst_file_send.clone();
            std::thread::spawn(move || {
                while let Ok(file) = src_file_recv.try_recv() {
                    let asset_file = OpenOptions::new().read(true).open(&file).unwrap();
                    let reader = BufReader::new(asset_file);
                    let compressed = zstd::encode_all(reader, COMPRESSION_LEVEL).unwrap();

                    let asset_name = file.file_name().unwrap();
                    let size = compressed.len() as u64;
                    let base = base_ptr.fetch_add(size, Ordering::Relaxed);

                    dst_file_send
                        .send(DstFile {
                            name: Utf8PathBuf::from_path_buf(asset_name.into()).unwrap(),
                            data: compressed,
                            base,
                            size,
                        })
                        .unwrap();
                }
            })
        })
        .collect::<Vec<_>>();

    let (mut writer, manifest) = manifest_builder.join().unwrap();
    writer_threads.into_iter().for_each(|t| t.join().unwrap());

    // Serialize and compress the manifest
    let manifest = bincode::serialize(&manifest).unwrap();
    let manifest = zstd::encode_all(Cursor::new(manifest), COMPRESSION_LEVEL).unwrap();

    writer
        .seek(SeekFrom::Start(base_ptr.load(Ordering::Relaxed)))
        .unwrap();
    let size = manifest.len() as u64;
    let base = writer.stream_position().unwrap();
    writer.write_all(&manifest).unwrap();

    // Update manifest base and size in file
    writer
        .seek(SeekFrom::Start(std::mem::size_of_val(&VERSION) as u64))
        .unwrap();
    writer.write_all(bytemuck::bytes_of(&base)).unwrap();
    writer.write_all(bytemuck::bytes_of(&size)).unwrap();
}
