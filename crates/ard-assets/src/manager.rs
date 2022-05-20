use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::BuildHasherDefault,
    num::NonZeroU32,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use crate::handle::Handle;
use crate::prelude::{
    AnyAssetLoader, Asset, AssetLoadResult, AssetLoader, AssetName, AssetNameBuf,
    AssetPostLoadResult, FolderPackage, Package, PackageInterface,
};
use ard_ecs::{id_map::FastIntHasher, prelude::*};
use crossbeam_utils::sync::{ShardedLock, ShardedLockReadGuard, ShardedLockWriteGuard};
use serde::{Deserialize, Serialize};

/// Asset manager.
#[derive(Resource, Clone)]
pub struct Assets(pub(crate) Arc<AssetsInner>);

pub(crate) struct AssetsInner {
    /// All the loaded packages.
    packages: Vec<Package>,
    /// Runtime for asyncronous asset loading.
    runtime: tokio::runtime::Runtime,
    /// ID counter for assets.
    asset_ids: AtomicU32,
    /// Map of all assets.
    pub(crate) assets: flurry::HashMap<u32, AssetData, BuildHasherDefault<FastIntHasher>>,
    /// Maps asset names to their id.
    name_to_id: flurry::HashMap<AssetNameBuf, u32, fxhash::FxBuildHasher>,
    /// Set of all available assets. Maps their asset names to the index of the package they should
    /// be loaded from.
    available: HashMap<AssetNameBuf, usize>,
    /// List of asset extensions registered with the manager.
    extensions: flurry::HashSet<String>,
    /// Loaders used to load assets. Maps from type ID of the asset to the loader.
    loaders: flurry::HashMap<TypeId, Arc<dyn AnyAssetLoader>>,
}

pub struct AssetReadHandle<'a, T: Asset> {
    _map_guard: flurry::Guard<'a>,
    _lock_guard: ShardedLockReadGuard<'a, Option<Box<dyn Any>>>,
    asset: &'a T,
}

pub struct AssetWriteHandle<'a, T: Asset> {
    _map_guard: flurry::Guard<'a>,
    _lock_guard: ShardedLockWriteGuard<'a, Option<Box<dyn Any>>>,
    asset: &'a mut T,
}

pub(crate) struct AssetData {
    /// Asset data.
    pub asset: ShardedLock<Option<Box<dyn Any>>>,
    /// Asset name.
    pub name: AssetNameBuf,
    /// Index of the package the asset was loaded from.
    pub package: usize,
    /// Version counter for an asset with this id.
    pub ver: AtomicU32,
    /// Type of data held in the asset.
    pub ty: TypeId,
    /// Number of outstanding handles to this asset. Each time a handle is created via a load, this
    /// value is incremented. When the last copy of a handle is dropped, this value is decremented.
    /// Only when this value reaches 0 does the asset get destroyed. A value of `None` indicates
    /// that the asset is persistant and should not be deleted.
    pub outstanding_handles: Option<AtomicU32>,
}

unsafe impl Send for AssetData {}
unsafe impl Sync for AssetData {}

/// A list of all packages.
#[derive(Serialize, Deserialize, Debug)]
pub struct PackageList {
    pub packages: Vec<String>,
}

impl Assets {
    /// Loads available assets from the packages list. Takes in a number of threads to use when
    /// loading assets from disk.
    pub fn new(thread_count: usize) -> Self {
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
                Err(_) => {}
            }
        }

        // Find all assets in every package, and shadow old assets.
        let mut available = HashMap::<AssetNameBuf, usize>::default();
        for (i, package) in packages.iter().enumerate() {
            for asset_name in package.manifest().assets.keys() {
                available.insert(asset_name.clone(), i);
            }
        }

        Self(Arc::new(AssetsInner {
            packages,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(thread_count)
                .build()
                .unwrap(),
            available,
            assets: Default::default(),
            extensions: Default::default(),
            loaders: Default::default(),
            name_to_id: Default::default(),
            asset_ids: AtomicU32::new(0),
        }))
    }

    /// Register a new asset type to load.
    ///
    /// # Panics
    /// Panics if the same asset type is already registered or an asset with the same extension
    /// is already registered.
    pub fn register<A: Asset + 'static>(&mut self, loader: A::Loader) {
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
        if !self.0.extensions.insert(A::EXTENSION.into(), &guard) {
            panic!(
                "asset type with extension '{}' already exists",
                A::EXTENSION
            );
        }
    }

    /// Get an asset via it's handle.
    ///
    /// If the asset doesn't exist or has not yet been loaded, `None` is returned.
    ///
    /// # Panics
    /// Panics if the asset type is incorrect.
    pub fn get<T: Asset + 'static>(&self, handle: &Handle<T>) -> Option<AssetReadHandle<T>> {
        // Retrieve asset data
        let map_guard = self.0.assets.guard();
        let asset_data = match self.0.assets.get(&handle.id(), &map_guard) {
            Some(asset_data) => asset_data,
            None => return None,
        };

        assert_eq!(asset_data.ty, TypeId::of::<T>());

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

        // This looks stupid, but here me out.
        // When acquiring an asset, we must guard the asset hash map and then lock the
        // particular asset we care about. Then, to make sure no one breaks anything, we must
        // return the guard and lock along with the reference to the asset.
        //
        // The problem is that we can't move out the guard and lock at the same time. The compiler
        // thinks that the lock holds a reference to the guard because they both share the same
        // lifetime. This prevents us from moving the guard because it could invalidate the
        // reference held by the lock.
        //
        // However, the lock doesn't actually hold a reference. They merely share a lifetime. This
        // transmute essentially removes the lifetime that ties the lock and guard together,
        // allowing us to move them.
        let new_lock_guard: ShardedLockReadGuard<Option<Box<dyn Any>>> =
            unsafe { std::mem::transmute_copy(&lock_guard) };

        std::mem::forget(lock_guard);

        Some(AssetReadHandle::<T> {
            _lock_guard: new_lock_guard,
            _map_guard: map_guard,
            asset,
        })
    }

    /// Get an asset mutably via it's handle.
    ///
    /// If the asset doesn't exist or has not yet been loaded, `None` is returned.
    ///
    /// # Panics
    /// Panics if the asset type is incorrect.
    pub fn get_mut<T: Asset + 'static>(&self, handle: &Handle<T>) -> Option<AssetWriteHandle<T>> {
        // Retrieve asset data
        let map_guard = self.0.assets.guard();
        let asset_data = match self.0.assets.get(&handle.id(), &map_guard) {
            Some(asset_data) => asset_data,
            None => return None,
        };

        assert_eq!(asset_data.ty, TypeId::of::<T>());

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

        // See the 'get' method for an explanation of this
        let new_lock_guard: ShardedLockWriteGuard<Option<Box<dyn Any>>> =
            unsafe { std::mem::transmute_copy(&lock_guard) };

        std::mem::forget(lock_guard);

        Some(AssetWriteHandle::<T> {
            _lock_guard: new_lock_guard,
            _map_guard: map_guard,
            asset,
        })
    }

    /// Tries to get a copy of an asset handle. Returns `None` if the asset has not been requested
    /// for load.
    pub fn get_handle<A: Asset + 'static>(&self, name: &AssetName) -> Option<Handle<A>> {
        // Asset must be from the available list
        if !self.0.available.contains_key(name) {
            // TODO: Use errors instead of panicking here
            panic!("attempt to get handle of non-existant asset");
        }

        let name_guard = self.0.name_to_id.guard();
        match self.0.name_to_id.get(name, &name_guard) {
            Some(id) => {
                // The name might have been added, but the asset data object might not have, so if
                // we found the ID we can just loop until we get the data.
                let asset_guard = self.0.assets.guard();
                let asset = loop {
                    match self.0.assets.get(id, &asset_guard) {
                        Some(asset) => break asset,
                        None => continue,
                    }
                };

                if let Some(outstanding) = asset.outstanding_handles.as_ref() {
                    // If we have 0 outstanding handles, then the asset is unloaded
                    if outstanding.load(Ordering::Relaxed) == 0 {
                        return None;
                    }

                    outstanding.fetch_add(1, Ordering::Relaxed);
                    Some(Handle::new(
                        *id,
                        NonZeroU32::new(asset.ver.load(Ordering::Relaxed)).unwrap(),
                        self.clone(),
                    ))
                } else {
                    Some(Handle::new(
                        *id,
                        NonZeroU32::new(asset.ver.load(Ordering::Relaxed)).unwrap(),
                        self.clone(),
                    ))
                }
            }
            None => None,
        }
    }

    /// Load an asset asyncronously. Returns a handle to the asset. You should use this when
    /// loading dependent assets.
    ///
    /// # Note
    /// Although this should be exceedingly rare, the asset pointed to by this handle is not
    /// guaranteed to have been loaded.
    pub async fn load_async<A: Asset + 'static>(&self, name: &AssetName) -> Handle<A> {
        // Get a handle for the asset and return if it already existed
        let (handle, needs_init) = self.get_or_make_handle(name);

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
    pub fn load<A: Asset + 'static>(&self, name: &AssetName) -> Handle<A> {
        // Get a handle for the asset and return if it already existed
        let (handle, needs_init) = self.get_or_make_handle(name);

        if !needs_init {
            return handle;
        }

        // Spawn a task to load the asset
        let req = LoadRequest::<A> {
            assets: self.clone(),
            handle: handle.clone(),
        };

        self.0.runtime.spawn(async move {
            load_asset::<A>(req).await;
        });

        handle
    }

    /// Helper function that gets a handle for an asset, or creates a new uninitialized asset data
    /// object if it doesn't yet exist. An additional boolean is returned indicating if the asset
    /// pointed to by the handle needs initialization.
    fn get_or_make_handle<A: Asset + 'static>(&self, name: &AssetName) -> (Handle<A>, bool) {
        // Check to see if the asset is loaded and we can copy a handle
        if let Some(handle) = self.get_handle(name) {
            return (handle, false);
        }

        // Get a new handle id for the asset
        let new_id = self.0.asset_ids.fetch_add(1, Ordering::Relaxed);

        // Try to insert the handle we just made into the "name to id" map. If we detect that
        // this asset is already in the list, then another thread must also be trying to load
        // this asset, in which case we can try to get the handle again.
        let guard = self.0.name_to_id.guard();
        let (actual_id, construct_handle) =
            match self.0.name_to_id.try_insert(name.into(), new_id, &guard) {
                // We successfully added our handle, so we are responsible for asset data init
                Ok(_) => (new_id, false),
                // If we succeed at this point, then we're good. We've got the handle.
                // If we fail, then the handle must have been dropped, so we just need to
                // update the outstanding handles counter by 1 and construct a handle manually.
                Err(actual_id) => match self.get_handle(name) {
                    Some(handle) => return (handle, false),
                    None => (*actual_id.current, true),
                },
            };
        std::mem::drop(guard);

        let guard = self.0.assets.guard();

        // Asset data exists so all we need to do is make the handle
        if construct_handle {
            let asset_data = self.0.assets.get(&actual_id, &guard).unwrap();
            let old_val = asset_data
                .outstanding_handles
                .as_ref()
                .unwrap()
                .fetch_add(1, Ordering::Relaxed);

            (
                Handle::new(
                    actual_id,
                    NonZeroU32::new(asset_data.ver.load(Ordering::Relaxed)).unwrap(),
                    self.clone(),
                ),
                // If the old outstanding handle reference is non-zero, then another thread must be
                // sending a load request, so we don't have to
                old_val != 0,
            )
        }
        // Asset doesn't exist, so it's up to us to initialize
        else {
            self.0.assets.insert(
                actual_id,
                AssetData {
                    asset: ShardedLock::new(None),
                    name: name.into(),
                    package: *self.0.available.get(name).unwrap(),
                    ver: AtomicU32::new(1),
                    ty: TypeId::of::<A>(),
                    outstanding_handles: Some(AtomicU32::new(1)),
                },
                &guard,
            );

            (self.get_handle(name).unwrap(), true)
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

    // Find the name of the asset and the package to load it from
    let (name, package) = {
        let guard = req.assets.0.assets.guard();
        let asset_data = req.assets.0.assets.get(&req.handle.id(), &guard).unwrap();

        (
            asset_data.name.clone(),
            req.assets.0.packages[asset_data.package].clone(),
        )
    };

    // Use the loader to load the asset
    let (asset, mut post_load) = match loader
        .load(req.assets.clone(), package.clone(), &name)
        .await
    {
        AssetLoadResult::Ok(asset) => (asset, false),
        AssetLoadResult::PostLoad(asset) => (asset, true),
        AssetLoadResult::Err => {
            println!("unable to load asset `{:?}`", &name);
            return;
        }
    };

    // Put the asset into the asset container
    {
        let guard = req.assets.0.assets.guard();
        let asset_data = req.assets.0.assets.get(&req.handle.id(), &guard).unwrap();
        *asset_data.asset.write().unwrap() = Some(Box::new(asset));
    }

    // Loop until post load is not needed
    while post_load {
        let handle = unsafe { req.handle.clone().transmute() };
        match loader
            .post_load(req.assets.clone(), package.clone(), handle)
            .await
        {
            AssetPostLoadResult::Ok => post_load = false,
            AssetPostLoadResult::PostLoad => {}
            AssetPostLoadResult::Err => {
                println!("unable to load asset `{:?}`", &name);
                return;
            }
        }
    }
}
