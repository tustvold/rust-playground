use derive_more::Display;

pub mod client;
pub mod user;

#[derive(Debug, Display)]
pub enum PolicyError {
    #[display(fmt = "Permission Denied")]
    PermissionDenied,
}
