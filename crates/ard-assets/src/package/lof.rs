use std::{
    fs::OpenOptions,
    io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use camino::Utf8PathBuf;
use crossbeam_utils::sync::{ShardedLock, ShardedLockReadGuard, ShardedLockWriteGuard};
use path_slash::PathBufExt;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

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
        contents.resize(*size as usize, 0);

        let mut file = self.0.file.lock().await;
        file.seek(SeekFrom::Start(*offset)).await.unwrap();
        file.read_exact(&mut contents).await.unwrap();
        let contents = async { zstd::decode_all(Cursor::new(contents)).unwrap() }.await;

        Ok(contents)
    }

    async fn read_str(&self, file: Utf8PathBuf) -> Result<String, PackageReadError> {
        // I would prefer not to have to heap allocate. Look into replacing
        let file: Utf8PathBuf = file
            .into_std_path_buf()
            .to_slash()
            .unwrap()
            .to_string()
            .into();
        let (offset, size) = match self.0.lof_manifest.assets.get(&file) {
            Some(props) => props,
            None => return Err(PackageReadError::DoesNotExist(file)),
        };

        let mut contents = Vec::with_capacity(*size as usize);
        contents.resize(*size as usize, 0);

        let mut file = self.0.file.lock().await;
        file.seek(SeekFrom::Start(*offset)).await.unwrap();
        file.read_exact(&mut contents).await.unwrap();

        let contents = async { zstd::decode_all(Cursor::new(contents)).unwrap() }.await;
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
