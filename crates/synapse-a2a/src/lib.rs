pub mod admin;
pub mod public;
pub mod registry;
pub mod types;

pub use admin::a2a_admin_router;
pub use public::a2a_public_router;
pub use registry::{A2aRegistry, RegisteredA2aAgent};
pub use types::{
    A2aCatalog, A2aCatalogEntry, A2aResolveResponse, RegisterA2aAgentRequest,
};
