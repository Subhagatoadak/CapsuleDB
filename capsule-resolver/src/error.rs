use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error(transparent)]
    Registry(#[from] capsule_registry::RegistryError),
}
