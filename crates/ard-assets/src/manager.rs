use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::BuildHasherDefault,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    ptr::NonNull,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};

use crate::prelude::{
    AnyAssetLoader, Asset, AssetLoadResult, AssetLoader, AssetName, AssetNameBuf,
    AssetPostLoadResult, FolderPackage, Package, PackageId, PackageInterface,
};
use crate::{handle::Handle, prelude::RawHandle};
use ard_ecs::{id_map::FastIntHasher, prelude::*};
use crossbeam_utils::sync::{ShardedLock, ShardedLockReadGuard, ShardedLockWriteGuard};
use dashmap::{mapref::one::Ref, DashMap};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

/// Asset manager.
#[derive(Resource, Clone)]
pub struct Assets(pub(crate) Arc<AssetsInner>);

pub(crate) struct AssetsInner {
    /// Tokio runtime for loading assets.
    runtime: tokio::runtime::Runtime,
    /// All the loaded packages.
    packages: Vec<Package>,
    /// Map of all assets. The index of the asset in this list corresponds to its id.
    pub(crate) assets: DashMap<u32, AssetData, BuildHasherDefault<FastIntHasher>>,
    /// Maps asset names to their id.
    name_to_id: DashMap<AssetNameBuf, u32>,
    /// Counter for unique asset ids.
    id_counter: AtomicU32,
    /// Map of asset extensions registered with the manager to the type of asset they represent.
    extensions: DashMap<String, TypeId>,
    /// Loaders used to load assets. Maps from type ID of the asset to the loader.
    loaders: DashMap<TypeId, Arc<dyn AnyAssetLoader>, BuildHasherDefault<FastIntHasher>>,
    /// Map of default asset handles. The key is the type id of the asset type.
    default_assets: DashMap<TypeId, RawHandle, BuildHasherDefault<FastIntHasher>>,
    /// Handles to tasks for assets that are being loaded.
    loading: DashMap<u32, JoinHandle<()>>,
}

pub struct AssetReadHandle<'a, T: Asset> {
    _ref: Ref<'a, u32, AssetData, BuildHasherDefault<FastIntHasher>>,
    _lock_guard: ShardedLockReadGuard<'static, Option<Box<dyn Any>>>,
    _handle: Handle<T>,
    asset: NonNull<T>,
}

pub struct AssetWriteHandle<'a, T: Asset> {
    _ref: Ref<'a, u32, AssetData, BuildHasherDefault<FastIntHasher>>,
    _lock_guard: ShardedLockWriteGuard<'static, Option<Box<dyn Any>>>,
    _handle: Handle<T>,
    asset: NonNull<T>,
}

pub struct AssetData {
    /// Asset data.
    pub(crate) asset: ShardedLock<Option<Box<dyn Any>>>,
    /// Asset name.
    pub(crate) name: AssetNameBuf,
    /// Index of the package the asset was loaded from.
    pub(crate) package: PackageId,
    /// Flag indicating that this asset is being loaded.
    pub(crate) loading: AtomicBool,
    /// Number of outstanding handles to this asset. Each time a handle is created via a load, this
    /// value is incremented. When the last copy of a handle is dropped, this value is decremented.
    /// Only when this value reaches 0 does the asset get destroyed. A value of `u32::MAX` means
    /// the asset is persistant (never dropped).
    pub(crate) outstanding_handles: AtomicU32,
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
        let contents = match std::fs::read_to_string(Path::new("./packages/packages.ron")) {
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
            let mut path: PathBuf = Path::new("./packages/").into();
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
        let name_to_id = DashMap::default();
        let assets = DashMap::default();

        for (asset, package) in available {
            let id = assets.len() as u32;
            name_to_id.insert(asset.clone(), id);
            assets.insert(
                id,
                AssetData {
                    asset: ShardedLock::new(None),
                    name: asset,
                    package: PackageId::from(package),
                    loading: AtomicBool::new(false),
                    outstanding_handles: AtomicU32::new(0),
                },
            );
        }

        let id_counter = AtomicU32::new(name_to_id.len() as u32);

        Self(Arc::new(AssetsInner {
            runtime: tokio::runtime::Builder::new_multi_thread()
                // TODO: Make configurable
                .worker_threads(8)
                .thread_name("asset-loading-thread")
                .build()
                .unwrap(),
            packages,
            assets,
            extensions: Default::default(),
            loaders: Default::default(),
            name_to_id,
            id_counter,
            default_assets: Default::default(),
            loading: Default::default(),
        }))
    }

    /// Register a new asset type to load.
    ///
    /// # Panics
    /// Panics if the same asset type is already registered or an asset with the same extension
    /// is already registered.
    pub fn register<A: Asset + 'static>(&self, loader: A::Loader) {
        // Make sure the loader doesn't already exist.
        if self
            .0
            .loaders
            .insert(TypeId::of::<A>(), Arc::new(loader))
            .is_some()
        {
            panic!("asset loader of same type already exists");
        }

        // Make sure an asset with the same extension hasn't already been registered.
        if self
            .0
            .extensions
            .insert(A::EXTENSION.into(), TypeId::of::<A>())
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
        let asset_data = self.0.assets.get(&handle.id()).unwrap();
        while asset_data.loading.load(Ordering::Relaxed) {
            std::hint::spin_loop();
        }
    }

    #[inline]
    pub fn assets(
        &self,
    ) -> dashmap::iter::Iter<'_, u32, AssetData, BuildHasherDefault<FastIntHasher>> {
        self.0.assets.iter()
    }

    #[inline]
    pub fn get_package_by_name(&self, name: &Path) -> Option<PackageId> {
        for (i, package) in self.0.packages.iter().enumerate() {
            if package.path() == name {
                return Some(PackageId::from(i));
            }
        }

        None
    }

    /// Get an asset via it's handle.
    ///
    /// If the asset doesn't exist or has not yet been loaded, `None` is returned.
    ///
    /// # Panics
    /// Panics if the asset type is incorrect.
    #[inline]
    pub fn get<'a, T: Asset + 'static>(
        &'a self,
        handle: &'a Handle<T>,
    ) -> Option<AssetReadHandle<'a, T>> {
        // Retrieve the asset data
        let asset_data = self.0.assets.get(&handle.id()).unwrap();

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

        let lock_guard = unsafe { std::mem::transmute(lock_guard) };

        Some(AssetReadHandle::<T> {
            _ref: asset_data,
            _lock_guard: lock_guard,
            _handle: handle.clone(),
            asset: NonNull::new(asset as *const T as *mut T).unwrap(),
        })
    }

    /// Get an asset mutably via it's handle.
    ///
    /// If the asset doesn't exist or has not yet been loaded, `None` is returned.
    ///
    /// # Panics
    /// Panics if the asset type is incorrect.
    #[inline]
    pub fn get_mut<'a, T: Asset + 'static>(
        &'a self,
        handle: &'a Handle<T>,
    ) -> Option<AssetWriteHandle<'a, T>> {
        // Retrieve the asset data
        let asset_data = self.0.assets.get(&handle.id()).unwrap();

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

        let lock_guard = unsafe { std::mem::transmute(lock_guard) };

        Some(AssetWriteHandle::<T> {
            _ref: asset_data,
            _lock_guard: lock_guard,
            _handle: handle.clone(),
            asset: NonNull::new(asset as *mut T).unwrap(),
        })
    }

    /// Tries to get a copy of an asset handle. Returns `None` if the asset has not been requested
    /// for load.
    #[inline]
    pub fn get_handle<A: Asset + 'static>(&self, name: &AssetName) -> Option<Handle<A>> {
        let id = self.get_id::<A>(name);
        let asset_data = self.0.assets.get(&id).unwrap();

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
        self.0
            .assets
            .get(&handle.id())
            .unwrap()
            .increment_handle_counter();

        // If there was a preexisting default asset, turn it back into a handle and drop it
        if let Some(old) = self
            .0
            .default_assets
            .insert(TypeId::of::<A>(), handle.raw())
        {
            std::mem::drop(Handle::<A>::new(old.id, self.clone()));
        }
    }

    /// Gets a copy of the default asset for a type. Returns `None` if it doesn't exist.
    #[inline]
    pub fn get_default<A: Asset + 'static>(&self) -> Option<Handle<A>> {
        match self.0.default_assets.get(&TypeId::of::<A>()) {
            Some(raw) => {
                self.0
                    .assets
                    .get(&raw.id)
                    .unwrap()
                    .increment_handle_counter();
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

        // We perform the normal load asyncronously, then we spawn a task for the post load so that
        // it can be completed in parallel (possibly) with other loading operations.
        let needs_post_load = load_asset::<A>(&req).await;
        if needs_post_load {
            self.0.loading.insert(
                handle.id(),
                self.0.runtime.spawn(async move {
                    post_load_asset::<A>(&req).await;
                }),
            );
        }

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

        // NOTE: It's safe to drop the join handle here because we're guaranteed to only have one
        // thread loading an asset at a time (see `get_and_mark_for_load`). This means the handle
        // being replaced points to a task that has already completed.
        self.0.loading.insert(
            handle.id(),
            self.0.runtime.spawn(async move {
                let needs_post_load = load_asset::<A>(&req).await;
                if needs_post_load {
                    post_load_asset::<A>(&req).await;
                }
            }),
        );

        handle
    }

    /// Submits a request to reload an asset.
    #[inline]
    pub fn reload<A: Asset + 'static>(&self, handle: &Handle<A>) {
        // No op if we're already loading
        let asset_data = self.0.assets.get(&handle.id()).unwrap();

        if asset_data
            .loading
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        // Spawn a task to load the asset
        let req = LoadRequest::<A> {
            assets: self.clone(),
            handle: handle.clone(),
        };

        // NOTE: It's safe to drop the join handle here because we're guaranteed to only have one
        // thread loading an asset at a time (see `get_and_mark_for_load`). This means the handle
        // being replaced points to a task that has already completed.
        self.0.loading.insert(
            handle.id(),
            self.0.runtime.spawn(async move {
                let needs_post_load = load_asset::<A>(&req).await;
                if needs_post_load {
                    post_load_asset::<A>(&req).await;
                }
            }),
        );
    }

    /// Checks if an asset has been loaded or not.
    #[inline]
    pub fn loaded(&self, name: &AssetName) -> bool {
        // Asset must be from the available list
        let id = *self.0.name_to_id.get(name).expect("asset does not exist");
        let asset_data = self.0.assets.get(&id).unwrap();
        let loaded = asset_data.asset.read().unwrap().is_some();
        loaded
    }

    /// Scans for a new asset by name. Returns `true` if the asset was found.
    #[inline]
    pub fn scan_for(&self, name: &AssetName) -> bool {
        // No-op if it already exists
        if self.0.name_to_id.contains_key(name) {
            return true;
        }

        // Scan packages in reverse order. The first package to have the asset is the one we care
        // about.
        for (i, package) in self.0.packages.iter().enumerate().rev() {
            if package.register_asset(name) {
                // Package contained the asset. Add it to our list
                let id = self.0.id_counter.fetch_add(1, Ordering::Relaxed);

                self.0.name_to_id.insert(AssetNameBuf::from(name), id);

                self.0.assets.insert(
                    id,
                    AssetData {
                        asset: ShardedLock::new(None),
                        name: AssetNameBuf::from(name),
                        package: PackageId::from(i),
                        loading: AtomicBool::new(false),
                        outstanding_handles: AtomicU32::new(0),
                    },
                );

                return true;
            }
        }

        false
    }

    /// Checks if a particular asset exists.
    #[inline]
    pub fn exists(&self, name: &AssetName) -> bool {
        self.0.name_to_id.contains_key(name)
    }

    /// Gets the type id of the asset associated with the extension of the asset name provided.
    #[inline]
    pub fn ty_id(&self, name: &AssetName) -> Option<TypeId> {
        let ext = match name.extension() {
            Some(ext) => ext,
            None => return None,
        };

        let ext = match ext.to_str() {
            Some(ext) => ext,
            None => return None,
        };

        self.0.extensions.get(ext).map(|id| *id)
    }

    /// Gets the name associated with the asset handle.
    #[inline]
    pub fn get_name<A: Asset + 'static>(&self, handle: &Handle<A>) -> AssetNameBuf {
        self.0.assets.get(&handle.id()).unwrap().name.clone()
    }

    #[inline]
    pub fn get_name_by_id(&self, id: u32) -> Option<AssetNameBuf> {
        self.0.assets.get(&id).map(|asset| asset.name.clone())
    }

    #[inline]
    pub fn get_id_by_name(&self, name: &AssetName) -> Option<u32> {
        self.0.name_to_id.get(name).map(|id| *id)
    }

    /// Helper function to verify that an asset name points to the correct type. Returns the id
    /// of the asset.
    #[inline]
    fn get_id<A: Asset + 'static>(&self, name: &AssetName) -> u32 {
        // Asset must be from the available list
        let id = *self.0.name_to_id.get(name).expect("asset does not exist");

        // First, ensure we have a loader for this type of asset and that the types match.
        let type_id = *self
            .0
            .extensions
            .get(
                name.extension()
                    .expect("asset has no extension")
                    .to_str()
                    .unwrap(),
            )
            .expect("no loader for the asset");
        assert_eq!(type_id, TypeId::of::<A>());

        id
    }

    /// Helper function that gets a handle for an asset An additional boolean is returned
    /// indicating if the asset pointed to by the handle needs loading.
    fn get_and_mark_for_load<A: Asset + 'static>(&self, name: &AssetName) -> (Handle<A>, bool) {
        let id = self.get_id::<A>(name);

        let asset_data = self.0.assets.get(&id).unwrap();

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
    #[inline(always)]
    pub fn name(&self) -> &AssetName {
        &self.name
    }

    #[inline(always)]
    pub fn package(&self) -> PackageId {
        self.package
    }

    /// Increments the outstand handles counter on the asset data object if the asset is not
    /// persistent.
    #[inline]
    pub(crate) fn increment_handle_counter(&self) {
        if self.outstanding_handles.load(Ordering::Relaxed) != u32::MAX {
            self.outstanding_handles.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Decrements the outstand handles counter on the asset data object if the asset is not
    /// persistent. If the asset needs to be dropped, `true` is returned.
    #[inline]
    pub(crate) fn decrement_handle_counter(&self) -> bool {
        if self.outstanding_handles.load(Ordering::Relaxed) != u32::MAX {
            (self.outstanding_handles.fetch_sub(1, Ordering::Relaxed) - 1) == 0
        } else {
            false
        }
    }
}

impl Drop for AssetsInner {
    fn drop(&mut self) {
        let _ctx = self.runtime.enter();
        for pair in self.loading.iter() {
            pair.value().abort();
        }
    }
}

impl<'a, T: Asset> Deref for AssetReadHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.asset.as_ref() }
    }
}

impl<'a, T: Asset> Deref for AssetWriteHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.asset.as_ref() }
    }
}

impl<'a, T: Asset> DerefMut for AssetWriteHandle<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.asset.as_mut() }
    }
}

struct LoadRequest<A: Asset> {
    assets: Assets,
    handle: Handle<A>,
}

/// Helper function to load assets asyncronously.
/// Returns a flag indicading if post load operations are required.
async fn load_asset<A: Asset + 'static>(req: &LoadRequest<A>) -> bool {
    use ard_log::*;

    let asset_data = req.assets.0.assets.get(&req.handle.id()).unwrap();

    // Find the loader for this asset type
    let loader = {
        match req.assets.0.loaders.get(&TypeId::of::<A>()) {
            Some(loader) => loader.clone(),
            None => {
                error!("loader for requested asset type does not exist");
                asset_data.loading.store(false, Ordering::Relaxed);
                return false;
            }
        }
    };

    let loader = loader.as_any().downcast_ref::<A::Loader>().unwrap();

    // Find the package to load it from
    let package = req.assets.0.packages[usize::from(asset_data.package)].clone();

    // Use the loader to load the asset
    let (asset, post_load, persistent) = match loader
        .load(req.assets.clone(), package.clone(), &asset_data.name)
        .await
    {
        Ok(res) => match res {
            AssetLoadResult::Loaded { asset, persistent } => (asset, false, persistent),
            AssetLoadResult::NeedsPostLoad { asset, persistent } => (asset, true, persistent),
        },
        Err(err) => {
            error!("error loading asset `{:?}` : {}", &asset_data.name, err);
            asset_data.loading.store(false, Ordering::Relaxed);
            return false;
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

    return post_load;
}

/// Helper function to perform post load operations on an asset
async fn post_load_asset<A: Asset + 'static>(req: &LoadRequest<A>) {
    use ard_log::*;

    let asset_data = req.assets.0.assets.get(&req.handle.id()).unwrap();

    // Find the loader for this asset type
    let loader = {
        match req.assets.0.loaders.get(&TypeId::of::<A>()) {
            Some(loader) => loader.clone(),
            None => {
                error!("loader for requested asset type does not exist");
                asset_data.loading.store(false, Ordering::Relaxed);
                return;
            }
        }
    };

    let loader = loader.as_any().downcast_ref::<A::Loader>().unwrap();

    // Find the package to load it from
    let package = req.assets.0.packages[usize::from(asset_data.package)].clone();

    let mut post_load = true;
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
                error!("error loading asset `{:?}` : {}", &asset_data.name, err);
                return;
            }
        }
    }
}
