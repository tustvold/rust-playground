use std::collections::HashSet;
use std::net::SocketAddr;

use rocket::request::Form;
use rocket::{Route, State};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};

use jwt::tag;
use rocket_util::UserAgent;
use telemetry::Measure;

use crate::api::error::ApiError;
use crate::api::ApiConfig;
use crate::model::{GrantType, Scope};
use crate::service::AuthService;
use std::sync::Arc;

lazy_static! {
    static ref TOKEN_MEASURE: Measure = Measure::new("controller", "token");
}

#[derive(Debug, Serialize, Deserialize, FromForm)]
struct TokenRequest {
    grant_type: GrantType,
    client_id: String,
    client_secret: Option<String>,
    device_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    expires_in: i64,
}

fn get_scopes(data: &TokenRequest) -> Result<HashSet<Scope>, ApiError> {
    if let Some(scope_str) = data.scope.as_ref() {
        return tag::parse_space_delimited(&scope_str).map_err(|_| ApiError::InvalidRequest);
    }
    Ok(Default::default())
}

fn get_device_name<'a>(user_agent: &'a Option<UserAgent>, data: &'a TokenRequest) -> &'a str {
    if let Some(d) = &data.device_name {
        d.as_str()
    } else if let Some(user_agent) = user_agent {
        user_agent.0.as_str()
    } else {
        "Unspecified"
    }
}

#[post("/api/v1/token", data = "<request>")]
async fn token(
    addr: Option<SocketAddr>,
    user_agent: Option<UserAgent>,
    auth: State<'_, Arc<AuthService>>,
    config: State<'_, ApiConfig>,
    request: Form<TokenRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    TOKEN_MEASURE
        .stats(async move {
            let scopes = get_scopes(&request.0)?;
            let authenticator = auth.get_authenticator(&request.0.client_id, &addr).await?;

            let authenticated = match request.grant_type {
                GrantType::Password => {
                    let username = request.username.as_ref().ok_or(ApiError::InvalidRequest)?;
                    let password = request.password.as_ref().ok_or(ApiError::InvalidRequest)?;
                    auth.auth_password(authenticator, &username, &password, scopes)
                        .await?
                }
                GrantType::ClientCredentials => {
                    let client_secret = request
                        .client_secret
                        .as_ref()
                        .ok_or(ApiError::InvalidRequest)?;
                    auth.auth_client_credential(authenticator, client_secret, scopes)
                        .await?
                }
                GrantType::RefreshToken => {
                    let refresh_token = request
                        .refresh_token
                        .as_ref()
                        .ok_or(ApiError::InvalidRequest)?;
                    auth.auth_refresh_token(authenticator, &refresh_token, scopes)
                        .await?
                }
            };

            let access_token = auth
                .generate_access_token(&authenticated, config.access_token_ttl)
                .await?;

            let device_name = get_device_name(&user_agent, &request);
            let refresh_token = auth
                .generate_renewal_token(authenticated, device_name, config.refresh_token_ttl)
                .await?;

            Ok(Json(TokenResponse {
                access_token,
                refresh_token,
                expires_in: config.access_token_ttl,
            }))
        })
        .await
}

pub(crate) fn routes() -> Vec<Route> {
    routes![token]
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use ring::rand::SystemRandom;
    use rocket::http::{ContentType, Status};

    use jwt::{Issuer, Validator};

    use crate::service::token::TokenService;

    use super::*;
    use crate::dao::{ClientDao, RenewalTokenDao, UserDao};
    use chrono::{Duration, Utc};

    struct State {
        validator: Validator,
        client_dao: Arc<dyn ClientDao>,
        user_dao: Arc<dyn UserDao>,
        renewal_dao: Arc<dyn RenewalTokenDao>,
        client: rocket::local::asynchronous::Client,
    }

    impl State {
        async fn new() -> State {
            let rand = Arc::new(SystemRandom::new());
            let token = Arc::new(TokenService::new(rand.clone()));
            let issuer = Arc::new(Issuer::test(rand).expect("Failed to setup issuer"));
            let validator = issuer.new_validator().expect("Failed to create validator");
            let user_dao = Arc::new(crate::dao::UserDaoMemory::new());
            let client_dao = Arc::new(crate::dao::ClientDaoMemory::new(token.clone()));
            let renewal_dao = Arc::new(crate::dao::RenewalTokenDaoMemory::new(token));

            let auth_service = Arc::new(AuthService::new(
                user_dao.clone(),
                client_dao.clone(),
                renewal_dao.clone(),
                issuer,
            ));

            let rocket = rocket::ignite()
                .manage(validator.clone())
                .manage(auth_service)
                .manage(ApiConfig::default())
                .manage(client_dao.clone() as Arc<dyn ClientDao>)
                .manage(renewal_dao.clone() as Arc<dyn RenewalTokenDao>)
                .manage(user_dao.clone() as Arc<dyn UserDao>)
                .mount("/", routes());

            let client = rocket::local::asynchronous::Client::untracked(rocket)
                .await
                .expect("valid rocket instance");

            State {
                validator,
                client_dao,
                user_dao,
                renewal_dao,
                client,
            }
        }

        async fn init_client(
            &self,
            client_scopes: HashSet<Scope>,
            grants: HashSet<GrantType>,
        ) -> Result<String, Box<dyn Error>> {
            let (client_id, _) = self
                .client_dao
                .register(
                    "my_client".to_string(),
                    client_scopes.clone(),
                    grants,
                    false,
                    false,
                    None,
                )
                .await?;

            Ok(client_id)
        }

        async fn refresh_req(
            &self,
            client_id: &str,
            token_scopes: &HashSet<Scope>,
            user_scopes: &HashSet<Scope>,
            expiry: i64,
        ) -> Result<TokenRequest, Box<dyn Error>> {
            let token = self
                .renewal_dao
                .generate(
                    "test_user_id",
                    &client_id,
                    "foo",
                    token_scopes.clone(),
                    Utc::now() + Duration::seconds(expiry),
                )
                .await?;

            let request = TokenRequest {
                grant_type: GrantType::RefreshToken,
                client_id: client_id.to_string(),
                client_secret: None,
                device_name: None,
                username: None,
                password: None,
                refresh_token: Some(token),
                scope: Some(tag::serialize_space_delimited(user_scopes.iter())),
            };
            Ok(request)
        }

        async fn password_req(
            &self,
            client_id: String,
            user_scopes: &HashSet<Scope>,
            req_scopes: &HashSet<Scope>,
            correct_password: bool,
        ) -> Result<TokenRequest, Box<dyn Error>> {
            let username = "fizbuz";
            let user_id = "test_user_id";
            let actual_password = "password123";
            let request_password = if correct_password {
                actual_password
            } else {
                "Incorrect"
            };

            self.user_dao
                .create_credential(username, user_id, actual_password, user_scopes.clone())
                .await?;

            Ok(TokenRequest {
                grant_type: GrantType::Password,
                client_id,
                client_secret: None,
                device_name: None,
                username: Some(username.to_string()),
                password: Some(request_password.to_string()),
                refresh_token: None,
                scope: Some(tag::serialize_space_delimited(req_scopes.iter())),
            })
        }

        async fn do_request(
            &self,
            request: &TokenRequest,
            status: Status,
        ) -> Option<TokenResponse> {
            let body = serde_urlencoded::to_string(request).expect("request must serialize");
            let response = self
                .client
                .post("/api/v1/token")
                .header(ContentType::Form)
                .body(body)
                .dispatch()
                .await;
            assert_eq!(response.status(), status);

            if status != Status::Ok {
                return None;
            }

            let body = response.into_bytes().await.unwrap();
            Some(serde_json::from_slice(&body).expect("failed to deserialize response"))
        }
    }

    #[tokio::test]
    async fn test_password() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;

        let scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password].iter().cloned().collect();
        let client_id = state.init_client(scopes.clone(), grants).await?;

        let request = state
            .password_req(client_id.clone(), &scopes, &scopes, true)
            .await?;
        let decoded = state.do_request(&request, Status::Ok).await.unwrap();

        assert!(decoded.refresh_token.is_none());
        let claims = state.validator.validate(&decoded.access_token)?;
        assert_eq!(claims.scopes, scopes);
        assert_eq!(claims.sub.as_ref().unwrap(), "test_user_id");
        assert_eq!(claims.cid, client_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_password_incorrect() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;
        let scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password].iter().cloned().collect();
        let client_id = state.init_client(scopes.clone(), grants).await?;

        let request = state
            .password_req(client_id.clone(), &scopes, &scopes, false)
            .await?;
        let data = state.do_request(&request, Status::BadRequest).await;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_password_illegal_client_scopes() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;
        let user_scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let client_scopes: HashSet<_> = Default::default();
        let grants: HashSet<_> = [GrantType::Password].iter().cloned().collect();
        let client_id = state.init_client(client_scopes.clone(), grants).await?;

        let request = state
            .password_req(client_id.clone(), &user_scopes, &user_scopes, true)
            .await?;
        let data = state.do_request(&request, Status::BadRequest).await;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_password_illegal_user_scopes() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;
        let user_scopes: HashSet<_> = Default::default();
        let request_scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let client_scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password].iter().cloned().collect();
        let client_id = state.init_client(client_scopes.clone(), grants).await?;

        let request = state
            .password_req(client_id.clone(), &user_scopes, &request_scopes, true)
            .await?;

        let data = state.do_request(&request, Status::BadRequest).await;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_illegal_grant() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;
        let user_scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let client_scopes: HashSet<_> = Default::default();
        let grants: HashSet<_> = Default::default();
        let client_id = state.init_client(client_scopes.clone(), grants).await?;

        let request = state
            .password_req(client_id.clone(), &user_scopes, &user_scopes, true)
            .await?;

        let data = state.do_request(&request, Status::BadRequest).await;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_password_offline() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;
        let user_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let client_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password].iter().cloned().collect();
        let client_id = state.init_client(client_scopes.clone(), grants).await?;

        let request = state
            .password_req(client_id.clone(), &user_scopes, &user_scopes, true)
            .await?;
        let decoded = state.do_request(&request, Status::Ok).await.unwrap();

        assert!(decoded.refresh_token.is_some());

        state
            .renewal_dao
            .consume(&client_id, decoded.refresh_token.as_ref().unwrap())
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_token() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;

        let user_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let client_scopes: HashSet<_> = [Scope::Superuser, Scope::OfflineAccess]
            .iter()
            .cloned()
            .collect();
        let grants: HashSet<_> = [GrantType::Password, GrantType::RefreshToken]
            .iter()
            .cloned()
            .collect();

        let client_id = state.init_client(client_scopes.clone(), grants).await?;

        let request = state
            .refresh_req(&client_id, &user_scopes, &user_scopes, 100000)
            .await?;

        let decoded = state.do_request(&request, Status::Ok).await.unwrap();
        assert!(decoded.refresh_token.is_some());

        state
            .renewal_dao
            .consume(&client_id, decoded.refresh_token.as_ref().unwrap())
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_token_expired() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;

        let user_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let client_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password, GrantType::RefreshToken]
            .iter()
            .cloned()
            .collect();

        let client_id = state.init_client(client_scopes.clone(), grants).await?;

        let request = state
            .refresh_req(&client_id, &user_scopes, &user_scopes, -100000)
            .await?;

        let data = state.do_request(&request, Status::Unauthorized).await;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_token_additional_scopes() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;
        let user_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let client_scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password, GrantType::RefreshToken]
            .iter()
            .cloned()
            .collect();

        let client_id = state.init_client(client_scopes.clone(), grants).await?;
        let request = state
            .refresh_req(&client_id, &Default::default(), &user_scopes, 100000)
            .await?;

        let data = state.do_request(&request, Status::BadRequest).await;
        assert!(data.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_client_credentials() -> Result<(), Box<dyn Error>> {
        let state = State::new().await;

        let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::ClientCredentials].iter().cloned().collect();

        let (client_id, token_opt) = state
            .client_dao
            .register(
                "my_client".to_string(),
                scopes.clone(),
                grants,
                true,
                false,
                None,
            )
            .await?;

        let token = token_opt.expect("no client credential");

        let request = TokenRequest {
            grant_type: GrantType::ClientCredentials,
            client_id: client_id.clone(),
            client_secret: Some(token),
            device_name: None,
            username: None,
            password: None,
            refresh_token: None,
            scope: Some(tag::serialize_space_delimited(scopes.iter())),
        };

        let decoded = state.do_request(&request, Status::Ok).await.unwrap();

        // Even though the client has the scope, we don't expect a refresh token
        assert!(decoded.refresh_token.is_none());

        let claims = state.validator.validate(&decoded.access_token)?;
        assert_eq!(claims.scopes, scopes);
        assert!(claims.sub.is_none());
        assert_eq!(claims.cid, client_id);

        Ok(())
    }
}
