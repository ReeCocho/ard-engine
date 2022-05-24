use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::BuildHasherDefault,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};

use crate::prelude::{
    AnyAssetLoader, Asset, AssetLoadResult, AssetLoader, AssetName, AssetNameBuf,
    AssetPostLoadResult, FolderPackage, Package, PackageInterface,
};
use crate::{handle::Handle, prelude::RawHandle};
use ard_ecs::{id_map::FastIntHasher, prelude::*};
use crossbeam_utils::sync::{ShardedLock, ShardedLockReadGuard, ShardedLockWriteGuard};
use serde::{Deserialize, Serialize};

/// Asset manager.
#[derive(Resource, Clone)]
pub struct Assets(pub(crate) Arc<AssetsInner>);

pub(crate) struct AssetsInner {
    /// All the loaded packages.
    packages: Vec<Package>,
    /// Map of all assets. The index of the asset in this list corresponds to its id.
    pub(crate) assets: Vec<AssetData>,
    /// Maps asset names to their id.
    name_to_id: HashMap<AssetNameBuf, u32, fxhash::FxBuildHasher>,
    /// Map of asset extensions registered with the manager to the type of asset they represent.
    extensions: flurry::HashMap<String, TypeId, fxhash::FxBuildHasher>,
    /// Loaders used to load assets. Maps from type ID of the asset to the loader.
    loaders: flurry::HashMap<TypeId, Arc<dyn AnyAssetLoader>, BuildHasherDefault<FastIntHasher>>,
    /// Map of default asset handles. The key is the type id of the asset type.
    default_assets: flurry::HashMap<TypeId, RawHandle, fxhash::FxBuildHasher>,
}

pub struct AssetReadHandle<'a, T: Asset> {
    _lock_guard: ShardedLockReadGuard<'a, Option<Box<dyn Any>>>,
    _handle: Handle<T>,
    asset: &'a T,
}

pub struct AssetWriteHandle<'a, T: Asset> {
    _lock_guard: ShardedLockWriteGuard<'a, Option<Box<dyn Any>>>,
    _handle: Handle<T>,
    asset: &'a mut T,
}

pub(crate) struct AssetData {
    /// Asset data.
    pub asset: ShardedLock<Option<Box<dyn Any>>>,
    /// Asset name.
    pub name: AssetNameBuf,
    /// Index of the package the asset was loaded from.
    pub package: usize,
    /// Flag indicating that this asset is being loaded.
    pub loading: AtomicBool,
    /// Number of outstanding handles to this asset. Each time a handle is created via a load, this
    /// value is incremented. When the last copy of a handle is dropped, this value is decremented.
    /// Only when this value reaches 0 does the asset get destroyed. A value of `u32::MAX` means
    /// the asset is persistant (never dropped).
    pub outstanding_handles: AtomicU32,
}

unsafe impl Send for AssetData {}
unsafe impl Sync for AssetData {}

/// A list of all packages.
#[derive(Serialize, Deserialize, Debug)]
pub struct PackageList {
    pub packages: Vec<String>,
}

impl Default for Assets {
    fn default() -> Self {
        Assets::new()
    }
}

impl Assets {
    /// Loads available assets from the packages list. Takes in a number of threads to use when
    /// loading assets from disk.
    pub fn new() -> Self {
        // Load the package list
        let contents = match std::fs::read_to_string(Path::new("./assets/packages.ron")) {
            Ok(contents) => contents,
            Err(_) => panic!("could not load packages list"),
        };

        let package_list = match ron::from_str::<PackageList>(&contents) {
            Ok(package_list) => package_list,
            Err(_) => panic!("packages list is invalid or corrupt"),
        };

        // Prune duplicates (slow, but this only happens once at startup, so whatever)
        let mut package_names = Vec::default();
        for package in package_list.packages {
            if !package_names.contains(&package) {
                package_names.push(package);
            }
        }

        // Attempt to open packages
        let mut packages = Vec::<Package>::default();
        for name in package_names {
            let mut path: PathBuf = Path::new("./assets/").into();
            path.extend(Path::new(&name));

            // TODO: Implement non-folder packages
            match FolderPackage::open(&path) {
                Ok(package) => packages.push(package.into()),
                Err(err) => println!("error loading package `{}` : {}", &name, err),
            }
        }

        // Find all assets in every package, and shadow old assets.
        let mut available = HashMap::<AssetNameBuf, usize>::default();
        for (i, package) in packages.iter().enumerate() {
            for asset_name in package.manifest().assets.keys() {
                available.insert(asset_name.clone(), i);
            }
        }

        // Make a vector for all the assets and make a mapping from thier names to their ids.
        let mut name_to_id = HashMap::default();
        let mut assets = Vec::with_capacity(available.len());

        for (asset, package) in available {
            name_to_id.insert(asset.clone(), assets.len() as u32);
            assets.push(AssetData {
                asset: ShardedLock::new(None),
                name: asset,
                package,
                loading: AtomicBool::new(false),
                outstanding_handles: AtomicU32::new(0),
            });
        }

        Self(Arc::new(AssetsInner {
            packages,
            assets,
            extensions: Default::default(),
            loaders: Default::default(),
            name_to_id,
            default_assets: Default::default(),
        }))
    }

    /// Register a new asset type to load.
    ///
    /// # Panics
    /// Panics if the same asset type is already registered or an asset with the same extension
    /// is already registered.
    pub fn register<A: Asset + 'static>(&self, loader: A::Loader) {
        // Make sure the loader doesn't already exist.
        let guard = self.0.loaders.guard();
        if self
            .0
            .loaders
            .insert(TypeId::of::<A>(), Arc::new(loader), &guard)
            .is_some()
        {
            panic!("asset loader of same type already exists");
        }
        std::mem::drop(guard);

        // Make sure an asset with the same extension hasn't already been registered.
        let guard = self.0.extensions.guard();
        if self
            .0
            .extensions
            .insert(A::EXTENSION.into(), TypeId::of::<A>(), &guard)
            .is_some()
        {
            panic!(
                "asset type with extension '{}' already exists",
                A::EXTENSION
            );
        }
    }

    /// Blocks until an asset has been loaded.
    #[inline]
    pub fn wait_for_load<T: Asset + 'static>(&self, handle: &Handle<T>) {
        let asset_data = &self.0.assets[handle.id() as usize];
        while asset_data.loading.load(Ordering::Relaxed) {
            std::hint::spin_loop();
        }
    }

    /// Get an asset via it's handle.
    ///
    /// If the asset doesn't exist or has not yet been loaded, `None` is returned.
    ///
    /// # Panics
    /// Panics if the asset type is incorrect.
    #[inline]
    pub fn get<T: Asset + 'static>(&self, handle: &Handle<T>) -> Option<AssetReadHandle<T>> {
        // Retrieve the asset data
        let asset_data = &self.0.assets[handle.id() as usize];

        // Lock the asset for reading
        let lock_guard = asset_data.asset.read().unwrap();
        let asset = match lock_guard.as_ref() {
            Some(asset) => unsafe {
                (asset.downcast_ref::<T>().unwrap() as *const T)
                    .as_ref()
                    .unwrap()
            },
            None => return None,
        };

        Some(AssetReadHandle::<T> {
            _lock_guard: lock_guard,
            _handle: handle.clone(),
            asset,
        })
    }

    /// Get an asset mutably via it's handle.
    ///
    /// If the asset doesn't exist or has not yet been loaded, `None` is returned.
    ///
    /// # Panics
    /// Panics if the asset type is incorrect.
    #[inline]
    pub fn get_mut<T: Asset + 'static>(&self, handle: &Handle<T>) -> Option<AssetWriteHandle<T>> {
        // Retrieve the asset data
        let asset_data = &self.0.assets[handle.id() as usize];

        // Lock the asset for reading
        let mut lock_guard = asset_data.asset.write().unwrap();
        let asset = match lock_guard.as_mut() {
            Some(asset) => unsafe {
                (asset.downcast_mut::<T>().unwrap() as *mut T)
                    .as_mut()
                    .unwrap()
            },
            None => return None,
        };

        Some(AssetWriteHandle::<T> {
            _lock_guard: lock_guard,
            _handle: handle.clone(),
            asset,
        })
    }

    /// Tries to get a copy of an asset handle. Returns `None` if the asset has not been requested
    /// for load.
    #[inline]
    pub fn get_handle<A: Asset + 'static>(&self, name: &AssetName) -> Option<Handle<A>> {
        let id = self.get_id::<A>(name);
        let asset_data = &self.0.assets[id as usize];

        // Asset either has to be loaded or be loading
        let asset = asset_data.asset.read().unwrap();
        if asset.is_some() || asset_data.loading.load(Ordering::Relaxed) {
            asset_data.increment_handle_counter();
            Some(Handle::new(id, self.clone()))
        } else {
            None
        }
    }

    /// Set the default asset for a type.
    #[inline]
    pub fn set_default<A: Asset + 'static>(&self, handle: Handle<A>) {
        // Increment the outstanding reference counter for the asset so that if it gets replaced it
        // will drop correctly
        self.0.assets[handle.id() as usize].increment_handle_counter();

        // If there was a preexisting default asset, turn it back into a handle and drop it
        let guard = self.0.default_assets.guard();
        if let Some(old) = self
            .0
            .default_assets
            .insert(TypeId::of::<A>(), handle.raw(), &guard)
        {
            std::mem::drop(Handle::<A>::new(old.id, self.clone()));
        }
    }

    /// Gets a copy of the default asset for a type. Returns `None` if it doesn't exist.
    #[inline]
    pub fn get_default<A: Asset + 'static>(&self) -> Option<Handle<A>> {
        let guard = self.0.default_assets.guard();
        match self.0.default_assets.get(&TypeId::of::<A>(), &guard) {
            Some(raw) => {
                self.0.assets[raw.id as usize].increment_handle_counter();
                Some(Handle::new(raw.id, self.clone()))
            }
            None => None,
        }
    }

    /// Gets a copy of an asset handle, or returns the default handle on a failure.
    ///
    /// # Panics
    /// Panics if there is no default asset for the asset type requested.
    ///
    /// Panics if the asset name points to an asset that exists, but is of the wrong type.
    #[inline]
    pub fn get_handle_or_default<A: Asset + 'static>(&self, name: &AssetName) -> Handle<A> {
        // If asset is real, we try to get the handle as per usual
        if self.0.name_to_id.get(name).is_some() {
            match self.get_handle::<A>(name) {
                Some(handle) => handle,
                None => self.get_default::<A>().expect("no default asset"),
            }
        }
        // Otherwise, we return the default
        else {
            self.get_default::<A>().expect("no default asset")
        }
    }

    /// Load an asset asyncronously. Returns a handle to the asset. You should use this when
    /// loading dependent assets.
    pub async fn load_async<A: Asset + 'static>(&self, name: &AssetName) -> Handle<A> {
        // Get a handle for the asset and return if it already existed
        let (handle, needs_init) = self.get_and_mark_for_load(name);

        if !needs_init {
            return handle;
        }

        // Load the asset
        let req = LoadRequest::<A> {
            assets: self.clone(),
            handle: handle.clone(),
        };

        load_asset::<A>(req).await;

        handle
    }

    /// Load an asset. Returns a handle to the asset.
    ///
    /// If the asset has not yet been loaded, a request will be made to load the asset.
    #[inline]
    pub fn load<A: Asset + 'static>(&self, name: &AssetName) -> Handle<A> {
        // Get a handle for the asset and return if it already existed
        let (handle, needs_init) = self.get_and_mark_for_load(name);

        if !needs_init {
            return handle;
        }

        // Spawn a task to load the asset
        let req = LoadRequest::<A> {
            assets: self.clone(),
            handle: handle.clone(),
        };

        tokio::spawn(async move {
            load_asset::<A>(req).await;
        });

        handle
    }

    /// Helper function to verify that an asset name points to the correct type. Returns the id
    /// of the asset.
    #[inline]
    fn get_id<A: Asset + 'static>(&self, name: &AssetName) -> u32 {
        // Asset must be from the available list
        let id = *self.0.name_to_id.get(name).expect("asset does not exist");

        // First, ensure we have a loader for this type of asset and that the types match.
        let guard = self.0.extensions.guard();
        let type_id = *self
            .0
            .extensions
            .get(
                name.extension()
                    .expect("asset has no extension")
                    .to_str()
                    .unwrap(),
                &guard,
            )
            .expect("no loader for the asset");
        assert_eq!(type_id, TypeId::of::<A>());

        id
    }

    /// Helper function that gets a handle for an asset An additional boolean is returned
    /// indicating if the asset pointed to by the handle needs loading.
    fn get_and_mark_for_load<A: Asset + 'static>(&self, name: &AssetName) -> (Handle<A>, bool) {
        let id = self.get_id::<A>(name);

        let asset_data = &self.0.assets[id as usize];

        // Asset either has to be loaded or be loading
        let asset = asset_data.asset.read().unwrap();
        let needs_load = if asset.is_some() || asset_data.loading.load(Ordering::Relaxed) {
            false
        }
        // Asset is not loaded, so we must try to mark it. If We fail to mark it which means
        // another thread must have marked it right before us. Therefore, we do not need to load
        else {
            asset_data
                .loading
                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        };

        // Update handle counter if needed
        asset_data.increment_handle_counter();
        (Handle::new(id, self.clone()), needs_load)
    }
}

impl AssetData {
    /// Increments the outstand handles counter on the asset data object if the asset is not
    /// persistent.
    #[inline]
    pub fn increment_handle_counter(&self) {
        if self.outstanding_handles.load(Ordering::Relaxed) != u32::MAX {
            self.outstanding_handles.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Decrements the outstand handles counter on the asset data object if the asset is not
    /// persistent. If the asset needs to be dropped, `true` is returned.
    #[inline]
    pub fn decrement_handle_counter(&self) -> bool {
        if self.outstanding_handles.load(Ordering::Relaxed) != u32::MAX {
            (self.outstanding_handles.fetch_sub(1, Ordering::Relaxed) - 1) == 0
        } else {
            false
        }
    }
}

impl<'a, T: Asset> Deref for AssetReadHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.asset
    }
}

impl<'a, T: Asset> Deref for AssetWriteHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.asset
    }
}

impl<'a, T: Asset> DerefMut for AssetWriteHandle<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.asset
    }
}

struct LoadRequest<A: Asset> {
    assets: Assets,
    handle: Handle<A>,
}

/// Helper function to load assets asyncronously.
async fn load_asset<A: Asset + 'static>(req: LoadRequest<A>) {
    // Find the loader for this asset type
    let loader = {
        let guard = req.assets.0.loaders.guard();
        match req.assets.0.loaders.get(&TypeId::of::<A>(), &guard) {
            Some(loader) => loader.clone(),
            None => panic!("loader for requested asset type does not exist"),
        }
    };

    let loader = loader.as_any().downcast_ref::<A::Loader>().unwrap();

    // Find the package to load it from
    let asset_data = &req.assets.0.assets[req.handle.id() as usize];
    let package = req.assets.0.packages[asset_data.package].clone();

    // Use the loader to load the asset
    let (asset, mut post_load, persistent) = match loader
        .load(req.assets.clone(), package.clone(), &asset_data.name)
        .await
    {
        Ok(res) => match res {
            AssetLoadResult::Loaded { asset, persistent } => (asset, false, persistent),
            AssetLoadResult::NeedsPostLoad { asset, persistent } => (asset, true, persistent),
        },
        Err(err) => {
            println!("error loading asset `{:?}` : {}", &asset_data.name, err);
            return;
        }
    };

    // Update to be persistent if requested
    if persistent {
        asset_data
            .outstanding_handles
            .store(u32::MAX, Ordering::Relaxed);
    }

    // Put the asset into the asset container
    *asset_data.asset.write().unwrap() = Some(Box::new(asset));

    // Loading is still technically occuring, but post load allows for access at this point
    asset_data.loading.store(false, Ordering::Relaxed);

    // Loop until post load is not needed
    while post_load {
        let handle = unsafe { req.handle.clone().transmute() };
        match loader
            .post_load(req.assets.clone(), package.clone(), handle)
            .await
        {
            Ok(res) => match res {
                AssetPostLoadResult::Loaded => post_load = false,
                AssetPostLoadResult::NeedsPostLoad => post_load = true,
            },
            Err(err) => {
                println!("error loading asset `{:?}` : {}", &asset_data.name, err);
                return;
            }
        }
    }
}
