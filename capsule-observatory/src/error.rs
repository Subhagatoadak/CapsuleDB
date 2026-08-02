use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObservatoryError {
    #[error(transparent)]
    Registry(#[from] capsule_registry::RegistryError),
}
