use crate::model::JwtClaims;
use crate::model::Scope;
use crate::policy::PolicyError;

fn default(user_id: &str, claims: &JwtClaims) -> Result<(), PolicyError> {
    if claims.scopes.contains(&Scope::Superuser) {
        return Ok(());
    }

    if let Some(sub) = &claims.sub {
        if sub == user_id {
            return Ok(());
        }
    }

    Err(PolicyError::PermissionDenied)
}

pub fn get(user_id: &str, claims: &JwtClaims) -> Result<(), PolicyError> {
    default(user_id, claims)
}

pub fn get_username(user_id: &str, claims: &JwtClaims) -> Result<(), PolicyError> {
    default(user_id, claims)
}

pub fn change_scopes(claims: &JwtClaims) -> Result<(), PolicyError> {
    if claims.scopes.contains(&Scope::Superuser) {
        return Ok(());
    }
    Err(PolicyError::PermissionDenied)
}
