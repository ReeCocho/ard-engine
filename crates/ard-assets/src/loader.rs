/// Implemented by types that load a particular asset. Type is the asset that this loader loads.
pub trait Loader<A> {
    fn load(&self, asset_name: &str) -> A;
}