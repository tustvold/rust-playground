use std::hash::Hash;
use std::str::FromStr;

pub use error::{IssuerError, ValidatorError};
pub use issuer::{Issuer, IssuerConfig};
pub use model::{DefaultClaims, Jwk, Jwks, JwtClaims, Scope};
pub use validator::{Validator, ValidatorConfig};

mod error;
mod issuer;
mod model;
pub mod tag;
mod validator;

pub fn extract_jwt<S: Sized + FromStr + Hash + Eq>(
    hdr: Option<&String>,
    validator: &Validator,
) -> Result<JwtClaims<S>, ValidatorError> {
    if let Some(auth) = hdr {
        if auth.len() <= 7 || !auth[..7].eq_ignore_ascii_case("bearer ") {
            return Err(ValidatorError::JwtMissing);
        }
        validator.validate(auth[7..].trim())
    } else {
        Err(ValidatorError::JwtMissing)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use chrono::{DateTime, Duration, Utc};
    use ring::rand::SystemRandom;

    use super::*;

    fn fuzzy_date(a: &DateTime<Utc>, b: &DateTime<Utc>) -> bool {
        let delta = a.timestamp() - b.timestamp();
        delta < 5 && delta > -5
    }

    #[test]
    fn test_valid() -> Result<(), Box<dyn std::error::Error>> {
        let rand = Arc::new(SystemRandom::new());
        let issuer = Issuer::test(rand)?;
        let validator = issuer.new_validator()?;

        let scopes: HashSet<_> = ["fiz".to_string(), "bar".to_string()]
            .iter()
            .cloned()
            .collect();

        let now = Utc::now();
        let ttl = Duration::seconds(123);

        let token = issuer.issue(
            Some("foo".to_string()),
            "client_id".to_string(),
            scopes.iter(),
            ttl,
        )?;

        let claims = validator.validate::<String>(&token)?;
        assert_eq!(claims.scopes, scopes);
        assert_eq!(claims.cid, "client_id");
        assert_eq!(claims.sub.unwrap(), "foo");
        assert!(fuzzy_date(&claims.iat, &now));
        assert!(fuzzy_date(&claims.exp, &(now + ttl)));

        Ok(())
    }

    #[test]
    fn test_expired() -> Result<(), Box<dyn std::error::Error>> {
        let rand = Arc::new(SystemRandom::new());
        let issuer = Issuer::test(rand)?;
        let validator = issuer.new_validator()?;

        let scopes: HashSet<_> = ["fiz".to_string(), "bar".to_string()]
            .iter()
            .cloned()
            .collect();

        let token = issuer.issue(
            Some("foo".to_string()),
            "client_id".to_string(),
            scopes.iter(),
            Duration::seconds(-1000),
        )?;

        println!("{}", token);

        match validator.validate::<String>(&token) {
            Err(ValidatorError::JwtExpired) => (),
            _ => panic!(),
        }

        Ok(())
    }

    #[test]
    fn test_invalid() -> Result<(), Box<dyn std::error::Error>> {
        let rand = Arc::new(SystemRandom::new());
        let issuer = Issuer::test(rand)?;
        let validator = issuer.new_validator()?;

        match validator.validate::<String>("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ") {
            Err(ValidatorError::ParseError) => (),
            _ => panic!(),
        }

        match validator.validate::<String>("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c") {
            Err(ValidatorError::DecodeError(_)) => (),
            _ => panic!(),
        }

        match validator.validate::<String>(" eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEiLCJqa3UiOiJodHRwOi8vbG9jYWxob3N0OjgwODAvLndlbGwta25vd24vandrcy5qc29uIn0.eyJleHAiOiIyMDIwLTA0LTAzVDA2OjAzOjAwLjgyOTUzOTA0MloiLCJpYXQiOiIyMDIwLTA0LTAzVDA2OjE5OjQwLjgyOTUzOTA0MloiLCJjaWQiOiJjbGllbnRfaWQiLCJzdWIiOiJmb28iLCJzY29wZXMiOiJzdXBlcnVzZXIgb2ZmbGluZV9hY2Nlc3MifQ.W6cAKpBI_sbrWnLHQoz_t91Wz249eLhs1b-XKgfatV1-PmuV_fFfu1JieeyvFaLaWMg6e0_Koz9fR9xqN62Laebe23ds6Rj5UvaAkczj2YEv9vG7LxIKNrJ-04V-KVycsX0WhQd70pU14lwTX1VkXAF-v5kONBkDOTDSjZFpDzISMFbrf4a9tEoYGlGeWQ1Xw1sqP46zrjT4osSiRnrxcy9gOc-d6-yE2Bwgc545XB7fpDjsiJCbdCfwW6XbCiVB2C1-XVc8DJzGF0exnoWrwBJvAI-LgN2xscny81Y6ryzpX6859XG7grhq_FRuDHUaBEQiB_jzHX_nkahzRJM7DQ") {
            Err(ValidatorError::DecodeError(_)) => (),
            _ => panic!()
        }

        match validator.validate::<String>("eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEiLCJqa3UiOiJodHRwOi8vbG9jYWxob3N0OjgwODAvLndlbGwta25vd24vandrcy5qc29uIn0.eyJleHAiOiIyMDIwLTA0LTAzVDA2OjAzOjAwLjgyOTUzOTA0MloiLCJpYXQiOiIyMDIwLTA0LTAzVDA2OjE5OjQwLjgyOTUzOTA0MloiLCJjaWQiOiJjbGllbnRfaWQiLCJzdWIiOiJmb28iLCJzY29wZXMiOiJzdXBlcnVzZXIgb2ZmbGluZV9hY2Nlc3MifQ.W6cAKpBI_sbrWnLHQoz_t91Wz249eLhs1b-XKgfatV1-PmuV_fFfu1JieeyvFaLaWMg6e0_Koz9fR9xqN62Laebe23ds6Rj5UvaAkczj2YEv9vG7LxIKNrJ-04V-KVycsX0WhQd70pU14lwTX1VkXAF-v5kONBkDOTDSjZFpDzISMFbrf4a9tEoYGlGeWQ1Xw1sqP46zrjT4osSiRnrxcy9gOc-d6-yE2Bwgc545XB7fpDjsiJCbdCfwW6XbCiVB2C1-XVc8DJzGF0exnoWrwBJvAI-LgN2xscny81Y6ryzpX6859XG7grhq_FRuDHUaBEQiB_jzHX_nkahzRJM7DQ") {
            Err(ValidatorError::JwtInvalid) => (),
            _ => panic!()
        }

        Ok(())
    }
}
