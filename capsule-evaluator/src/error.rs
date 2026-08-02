use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvaluatorError {
    #[error(transparent)]
    Registry(#[from] capsule_registry::RegistryError),
}
