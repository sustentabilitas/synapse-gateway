pub mod registry;
pub mod types;
// admin, public added in later tasks

pub use registry::{expiry_from_ttl, A2aRegistry, RegisteredA2aAgent};
pub use types::{
    A2aCatalog, A2aCatalogEntry, A2aResolveResponse, RegisterA2aAgentRequest,
};
