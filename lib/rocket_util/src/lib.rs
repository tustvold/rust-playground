use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::Request;

use jwt::{DefaultClaims, Validator, ValidatorError};
use rocket::figment::{providers::Env, Figment};

pub struct Authenticated {
    pub header: String,
    pub claims: DefaultClaims,
}

#[derive(Debug)]
pub enum AuthenticatedError {
    JwtMissing,
    JwtExpired,
    JwtInvalid,
    Internal,
}

#[rocket::async_trait]
impl<'a, 'r> FromRequest<'a, 'r> for Authenticated {
    type Error = AuthenticatedError;

    async fn from_request(request: &'a Request<'r>) -> Outcome<Authenticated, Self::Error> {
        let validator = request
            .managed_state::<Validator>()
            .expect("No validator registered");
        if let Some(auth) = request.headers().get_one("authorization") {
            if auth.len() <= 7 || !auth[..7].eq_ignore_ascii_case("bearer ") {
                return Outcome::Failure((Status::Unauthorized, AuthenticatedError::JwtMissing));
            }
            match validator.validate(auth[7..].trim()) {
                Ok(claims) => Outcome::Success(Authenticated {
                    header: auth.to_string(), // TODO: Avoid this copy
                    claims,
                }),
                Err(ValidatorError::JwtExpired) => {
                    Outcome::Failure((Status::Unauthorized, AuthenticatedError::JwtExpired))
                }
                Err(ValidatorError::ParseError)
                | Err(ValidatorError::JwtInvalid)
                | Err(ValidatorError::DecodeError(_))
                | Err(ValidatorError::JwtMissing) => {
                    Outcome::Failure((Status::BadRequest, AuthenticatedError::JwtInvalid))
                }
                Err(ValidatorError::ConfigError(_)) => {
                    Outcome::Failure((Status::InternalServerError, AuthenticatedError::Internal))
                }
            }
        } else {
            Outcome::Failure((Status::Unauthorized, AuthenticatedError::JwtMissing))
        }
    }
}

#[derive(Debug)]
pub struct UserAgent(pub String);

#[rocket::async_trait]
impl<'a, 'r> FromRequest<'a, 'r> for UserAgent {
    type Error = ();

    async fn from_request(request: &'a Request<'r>) -> Outcome<Self, Self::Error> {
        if let Some(agent) = request.headers().get_one("User-Agent") {
            return Outcome::Success(UserAgent(agent.to_string()));
        }
        Outcome::Forward(())
    }
}

pub fn figment() -> Figment {
    rocket::Config::figment()
        .merge(Env::prefixed("APP_").map(|s| s.as_str().replacen('_', ".", 1).into()))
}
