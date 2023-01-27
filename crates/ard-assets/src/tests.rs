use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    },
    time::Duration,
};

use crate::prelude::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct TestAsset {
    name: String,
    age: i32,
    is_alive: bool,
}

struct TestAssetLoader;

impl Asset for TestAsset {
    const EXTENSION: &'static str = "dat";

    type Loader = TestAssetLoader;
}

#[async_trait]
impl AssetLoader for TestAssetLoader {
    type Asset = TestAsset;

    async fn load(
        &self,
        _: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        let data = package.read_str(asset).await?;

        match ron::from_str::<TestAsset>(&data) {
            Ok(asset) => Ok(AssetLoadResult::Loaded {
                asset,
                persistent: false,
            }),
            Err(_) => Err(AssetLoadError::Unknown),
        }
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        panic!("post load not needed")
    }
}

struct DropSignalAsset {
    signal: Arc<AtomicBool>,
}

struct DropSignalAssetLoader;

impl Asset for DropSignalAsset {
    const EXTENSION: &'static str = "drp";

    type Loader = DropSignalAssetLoader;
}

impl Drop for DropSignalAsset {
    fn drop(&mut self) {
        self.signal.store(true, Ordering::Relaxed);
    }
}

#[async_trait]
impl AssetLoader for DropSignalAssetLoader {
    type Asset = DropSignalAsset;

    async fn load(
        &self,
        _: Assets,
        _: Package,
        _: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        Ok(AssetLoadResult::Loaded {
            asset: DropSignalAsset {
                signal: Arc::new(AtomicBool::new(false)),
            },
            persistent: false,
        })
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        panic!("post load not needed")
    }
}

struct PersistentAsset;

impl Asset for PersistentAsset {
    const EXTENSION: &'static str = "per";

    type Loader = PersistentAssetLoader;
}

struct PersistentAssetLoader;

#[async_trait]
impl AssetLoader for PersistentAssetLoader {
    type Asset = PersistentAsset;

    async fn load(
        &self,
        _: Assets,
        _: Package,
        _: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        Ok(AssetLoadResult::Loaded {
            asset: PersistentAsset,
            persistent: true,
        })
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        panic!("post load not needed")
    }
}

/// Basic test to check that assets get loaded in.
#[test]
fn asset_loading() {
    let assets = Assets::new();
    assets.register::<TestAsset>(TestAssetLoader);

    let asset = assets.load::<TestAsset>(AssetName::new("test_file.dat"));

    assets.wait_for_load(&asset);

    let asset = assets.get(&asset).unwrap();

    assert_eq!(asset.name, "Bob");
    assert_eq!(asset.age, 21);
    assert_eq!(asset.is_alive, true);
}

/// Tests that asset shadowing works. That is, between two packages with the same asset, the
/// package furthest in the load order will have it's asset loaded.
#[test]
fn shadowing() {
    let assets = Assets::new();
    assets.register::<TestAsset>(TestAssetLoader);

    let asset = assets.load::<TestAsset>(AssetName::new("shadowed.dat"));
    assets.wait_for_load(&asset);

    let asset = assets.get(&asset).unwrap();

    assert_eq!(asset.name, "Alice");
    assert_eq!(asset.age, 30);
    assert_eq!(asset.is_alive, true);
}

/// Tests that assets are dropped when they have no more references.
#[test]
fn ref_counting() {
    let assets = Assets::new();
    assets.register::<DropSignalAsset>(DropSignalAssetLoader);

    let handle = assets.load::<DropSignalAsset>(AssetName::new("dummy.drp"));
    assets.wait_for_load(&handle);

    let asset = assets.get(&handle).unwrap();
    let drop_signal = asset.signal.clone();
    std::mem::drop(asset);
    std::mem::drop(handle);

    // We need to wait here because the asset loader holds onto a handle to the asset and might not
    // be finished by the time we get here
    std::thread::sleep(Duration::from_millis(150));

    assert_eq!(drop_signal.load(Ordering::Relaxed), true);
}

/// Tests that default asset works and drop as expected.
#[test]
fn default_assets() {
    let assets = Assets::new();
    assets.register::<DropSignalAsset>(DropSignalAssetLoader);

    let handle = assets.load::<DropSignalAsset>(AssetName::new("dummy.drp"));
    assets.wait_for_load(&handle);

    let drop_signal = assets.get(&handle).unwrap().signal.clone();

    // Set the asset as default
    assets.set_default::<DropSignalAsset>(handle.clone());

    // Try to get an asset that doesn't exist. It must be equal to the default asset
    assert!(
        assets.get_handle_or_default::<DropSignalAsset>(AssetName::new("dummy_dne.drp")) == handle
    );

    // Load a new asset
    let new_handle = assets.load::<DropSignalAsset>(AssetName::new("dummy2.drp"));
    assets.wait_for_load(&new_handle);

    let drop_signal_new = assets.get(&new_handle).unwrap().signal.clone();

    // Set as the new default
    assets.set_default::<DropSignalAsset>(new_handle);

    assert_eq!(drop_signal.load(Ordering::Relaxed), false);
    std::mem::drop(handle);
    assert_eq!(drop_signal.load(Ordering::Relaxed), true);
    assert_eq!(drop_signal_new.load(Ordering::Relaxed), false);
}

/// Tests that when many threads attempt to load the same asset, they should all get the same
/// handle.
#[test]
fn many_requests() {
    // This test is non-determenistic, so we need to test multiple times.
    const THREAD_COUNT: usize = 10;
    const ITER_COUNT: usize = 1_000;

    for _ in 0..ITER_COUNT {
        let assets = Assets::new();
        assets.register::<TestAsset>(TestAssetLoader);
        assets.register::<DropSignalAsset>(DropSignalAssetLoader);

        let mut handles = Vec::with_capacity(THREAD_COUNT);
        let barrier = Arc::new(Barrier::new(THREAD_COUNT));

        for _ in 0..THREAD_COUNT {
            let c = Arc::clone(&barrier);
            let assets_clone = assets.clone();

            handles.push(std::thread::spawn(move || {
                c.wait();
                let h1 = assets_clone.load::<TestAsset>(AssetName::new("test_file.dat"));
                let h2 = assets_clone.load::<DropSignalAsset>(AssetName::new("dummy.drp"));
                assets_clone.wait_for_load(&h1);
                assets_clone.wait_for_load(&h2);
            }));
        }

        let mut asset_handles = Vec::with_capacity(THREAD_COUNT);
        for handle in handles {
            asset_handles.push(handle.join().unwrap());
        }

        for i in 1..THREAD_COUNT {
            assert!(asset_handles[0] == asset_handles[i]);
        }
    }
}

/// Test for when an asset is loaded and then dropped in one thread but also loaded in another.
#[test]
fn load_drop() {
    // This test is non-determenistic, so we need to test multiple times.
    const ITER_COUNT: usize = 1_000;

    for _ in 0..ITER_COUNT {
        let assets = Assets::new();
        assets.register::<TestAsset>(TestAssetLoader);

        let mut handles = Vec::with_capacity(2);
        let barrier = Arc::new(Barrier::new(2));

        let c = Arc::clone(&barrier);
        let assets_clone = assets.clone();
        handles.push(std::thread::spawn(move || {
            // Load the asset in one thread
            let handle = assets_clone.load::<TestAsset>(AssetName::new("test_file.dat"));

            // Wait for the asset loader to finish
            assets_clone.wait_for_load(&handle);

            // Wait for the second thread to start up
            c.wait();

            // Drop the asset
            std::mem::drop(handle);
        }));

        let c = Arc::clone(&barrier);
        let assets_clone = assets.clone();
        handles.push(std::thread::spawn(move || {
            // Wait for the first thread to be ready
            c.wait();

            // Load the asset
            let handle = assets_clone.load::<TestAsset>(AssetName::new("test_file.dat"));
            assets_clone.wait_for_load(&handle);
        }));

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

/// Test to check persistent assets.
#[test]
fn persistent_assets() {
    let assets = Assets::new();
    assets.register::<PersistentAsset>(PersistentAssetLoader);

    // Load the asset
    let asset = assets.load::<PersistentAsset>(AssetName::new("persistent.per"));
    assets.wait_for_load(&asset);

    // Ensure it is loaded correctly
    assets.get(&asset).unwrap();

    // Drop the handle. If it were not persistent, the asset would drop now
    std::mem::drop(asset);

    // If the asset were dropped, this would return `None` and fail
    assets
        .get_handle::<PersistentAsset>(AssetName::new("persistent.per"))
        .unwrap();
}
