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
    ) -> AssetLoadResult<Self::Asset> {
        let data = match package.read_str(asset).await {
            Ok(data) => data,
            Err(_) => return AssetLoadResult::Err,
        };

        match ron::from_str::<TestAsset>(&data) {
            Ok(asset) => AssetLoadResult::Ok(asset),
            Err(_) => AssetLoadResult::Err,
        }
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> AssetPostLoadResult {
        panic!("post load not needed")
    }
}

#[test]
fn asset_loading() {
    let mut assets = Assets::new(2);
    assets.register::<TestAsset>(TestAssetLoader);

    let asset = assets.load::<TestAsset>(AssetName::new("test_file.dat"));
    while assets.get(&asset).is_none() {}

    let asset = assets.get(&asset).unwrap();

    assert_eq!(asset.name, "Bob");
    assert_eq!(asset.age, 21);
    assert_eq!(asset.is_alive, true);
}
