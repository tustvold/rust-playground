use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{Duration, Utc};

use jwt::{Issuer, IssuerError};
use telemetry::{IsErr, Measure};

use crate::dao::{ClientDao, DaoError, RenewalTokenDao, UserDao};
use crate::model::{Client, Scope};

lazy_static! {
    static ref GET_AUTHENTICATOR_MEASURE: Measure =
        Measure::new("service", "auth_service_get_authenticator");
    static ref AUTH_PASSWORD_MEASURE: Measure =
        Measure::new("service", "auth_service_auth_password");
    static ref AUTH_REFRESH_TOKEN_MEASURE: Measure =
        Measure::new("service", "auth_service_auth_refresh_token");
    static ref AUTH_CLIENT_CREDENTIAL_MEASURE: Measure =
        Measure::new("service", "auth_service_auth_client_credential");
    static ref GENERATE_RENEWAL_TOKEN_MEASURE: Measure =
        Measure::new("service", "auth_service_generate_renewal_token");
}

pub enum AuthError {
    NotFound,
    NotLoopback,
    IllegalScopes,
    InvalidCredential,
    AlreadyExists,
    ExpiredCredential,
    InternalError(String),
}

impl IsErr for AuthError {
    fn is_err(&self) -> bool {
        matches!(self, AuthError::InternalError(_))
    }
}

impl From<DaoError> for AuthError {
    fn from(e: DaoError) -> Self {
        match e {
            DaoError::InvalidCredential => Self::InvalidCredential,
            DaoError::NotFound => Self::NotFound,
            DaoError::ExpiredCredential => Self::ExpiredCredential,
            DaoError::AlreadyExists => Self::AlreadyExists,
            DaoError::InternalError(e) => Self::InternalError(format!("DaoError: {}", e)),
        }
    }
}

impl From<IssuerError> for AuthError {
    fn from(e: IssuerError) -> Self {
        AuthError::InternalError(format!("IssuerError: {}", e))
    }
}

pub struct AuthService {
    user_dao: Arc<dyn UserDao>,
    client_dao: Arc<dyn ClientDao>,
    renewal_dao: Arc<dyn RenewalTokenDao>,
    issuer: Arc<Issuer>,
}

pub struct Authenticator {
    client: Client,
}

pub struct Authenticated {
    client_id: String,
    subject: Option<String>,
    scopes: HashSet<Scope>,
}

impl AuthService {
    pub fn new(
        user_dao: Arc<dyn UserDao>,
        client_dao: Arc<dyn ClientDao>,
        renewal_dao: Arc<dyn RenewalTokenDao>,
        issuer: Arc<Issuer>,
    ) -> AuthService {
        AuthService {
            user_dao,
            client_dao,
            renewal_dao,
            issuer,
        }
    }

    pub async fn get_authenticator(
        &self,
        client_id: &str,
        addr: &Option<SocketAddr>,
    ) -> Result<Authenticator, AuthError> {
        GET_AUTHENTICATOR_MEASURE
            .stats(async move {
                let client = self
                    .client_dao
                    .lookup(client_id)
                    .await?
                    .ok_or(AuthError::NotFound)?;

                if client.loopback && !addr.map_or(false, |x| x.ip().is_loopback()) {
                    return Err(AuthError::NotLoopback);
                }

                Ok(Authenticator { client })
            })
            .await
    }

    pub async fn auth_password(
        &self,
        client: Authenticator,
        username: &str,
        password: &str,
        scopes: HashSet<Scope>,
    ) -> Result<Authenticated, AuthError> {
        AUTH_PASSWORD_MEASURE
            .stats(async move {
                let user = self
                    .user_dao
                    .verify(username, password)
                    .await
                    .map_err(AuthError::from)?;

                if scopes.difference(&client.client.scopes).next().is_some()
                    || scopes.difference(&user.scopes).next().is_some()
                {
                    return Err(AuthError::IllegalScopes);
                }

                Ok(Authenticated {
                    subject: Some(user.user_id),
                    client_id: client.client.client_id,
                    scopes,
                })
            })
            .await
    }

    pub async fn auth_refresh_token(
        &self,
        client: Authenticator,
        token: &str,
        scopes: HashSet<Scope>,
    ) -> Result<Authenticated, AuthError> {
        AUTH_REFRESH_TOKEN_MEASURE
            .stats(async move {
                let refresh_token = self
                    .renewal_dao
                    .consume(&client.client.client_id, &token)
                    .await?;

                if scopes.is_empty() {
                    if refresh_token
                        .scopes
                        .difference(&client.client.scopes)
                        .next()
                        .is_some()
                    {
                        return Err(AuthError::IllegalScopes);
                    }

                    return Ok(Authenticated {
                        subject: Some(refresh_token.subject),
                        client_id: client.client.client_id,
                        scopes: refresh_token.scopes,
                    });
                }

                if scopes.difference(&client.client.scopes).next().is_some()
                    || scopes.difference(&refresh_token.scopes).next().is_some()
                {
                    return Err(AuthError::IllegalScopes);
                }

                Ok(Authenticated {
                    subject: Some(refresh_token.subject),
                    client_id: client.client.client_id,
                    scopes,
                })
            })
            .await
    }

    pub async fn auth_client_credential(
        &self,
        client: Authenticator,
        secret: &str,
        scopes: HashSet<Scope>,
    ) -> Result<Authenticated, AuthError> {
        AUTH_CLIENT_CREDENTIAL_MEASURE
            .stats(async move {
                let hashed_credential = client
                    .client
                    .credential
                    .as_ref()
                    .ok_or(AuthError::InvalidCredential)?;

                self.client_dao
                    .verify(
                        &client.client.client_id,
                        secret,
                        hashed_credential.as_slice(),
                    )
                    .await
                    .map_err(AuthError::from)?;

                if scopes.difference(&client.client.scopes).next().is_some() {
                    return Err(AuthError::IllegalScopes);
                }

                Ok(Authenticated {
                    subject: None,
                    client_id: client.client.client_id,
                    scopes,
                })
            })
            .await
    }

    pub async fn generate_access_token(
        &self,
        authenticated: &Authenticated,
        expiry: i64,
    ) -> Result<String, AuthError> {
        let access_token = self.issuer.issue(
            authenticated.subject.clone(),
            authenticated.client_id.clone(),
            authenticated.scopes.iter(),
            Duration::seconds(expiry),
        )?;

        Ok(access_token)
    }

    pub async fn generate_renewal_token(
        &self,
        authenticated: Authenticated,
        device_name: &str,
        expiry: i64,
    ) -> Result<Option<String>, AuthError> {
        let scopes = authenticated.scopes;
        let client_id = authenticated.client_id;

        if !scopes.contains(&Scope::OfflineAccess) {
            return Ok(None);
        }

        if let Some(subject) = authenticated.subject.as_ref() {
            let token = GENERATE_RENEWAL_TOKEN_MEASURE
                .stats(async move {
                    self.renewal_dao
                        .generate(
                            subject,
                            &client_id,
                            device_name,
                            scopes,
                            Utc::now() + Duration::seconds(expiry),
                        )
                        .await
                        .map_err(AuthError::from)
                })
                .await?;

            return Ok(Some(token));
        }

        Ok(None)
    }
}
