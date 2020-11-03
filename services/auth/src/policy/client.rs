use crate::model::JwtClaims;
use crate::model::Scope;
use crate::policy::PolicyError;

fn default(claims: &JwtClaims) -> Result<(), PolicyError> {
    if claims.scopes.contains(&Scope::Superuser) {
        return Ok(());
    }

    Err(PolicyError::PermissionDenied)
}

pub fn register(claims: &JwtClaims) -> Result<(), PolicyError> {
    default(claims)
}

pub fn get(claims: &JwtClaims) -> Result<(), PolicyError> {
    default(claims)
}

pub fn update(claims: &JwtClaims) -> Result<(), PolicyError> {
    default(claims)
}
