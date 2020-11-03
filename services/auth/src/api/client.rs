use std::collections::HashSet;
use std::sync::Arc;

use rocket::http::Status;
use rocket::{Route, State};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};

use rocket_util::Authenticated;
use telemetry::Measure;

use crate::api::error::ApiError;
use crate::dao::ClientDao;
use crate::model::{GrantType, Scope};
use crate::policy;

lazy_static! {
    static ref REGISTER_MEASURE: Measure = Measure::new("controller", "client_register");
    static ref GET_MEASURE: Measure = Measure::new("controller", "client_get");
    static ref UPDATE_MEASURE: Measure = Measure::new("controller", "client_update");
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateClientRequest {
    client_name: String,
    scopes: HashSet<Scope>,
    grants: HashSet<GrantType>,
    loopback: Option<bool>,
    credential: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateClientResponse {
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_credential: Option<String>,
}

#[post("/api/v1/client", data = "<form>")]
async fn register(
    authenticated: Authenticated,
    form: Json<CreateClientRequest>,
    client_dao: State<'_, Arc<dyn ClientDao>>,
) -> Result<Json<CreateClientResponse>, ApiError> {
    REGISTER_MEASURE
        .stats(async move {
            policy::client::register(&authenticated.claims)?;
            let request = form.into_inner();

            let (client_id, client_credential) = client_dao
                .register(
                    request.client_name,
                    request.scopes,
                    request.grants,
                    request.credential.unwrap_or(false),
                    request.loopback.unwrap_or(false),
                    None,
                )
                .await?;

            Ok(Json(CreateClientResponse {
                client_id,
                client_credential,
            }))
        })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientResponse {
    client_id: String,
    client_name: String,
    scopes: HashSet<Scope>,
    grants: HashSet<GrantType>,
}

#[get("/api/v1/client/<client_id>")]
async fn get(
    client_id: String,
    authenticated: Authenticated,
    client_dao: State<'_, Arc<dyn ClientDao>>,
) -> Result<Json<ClientResponse>, ApiError> {
    GET_MEASURE
        .stats(async move {
            policy::client::get(&authenticated.claims).map_err(ApiError::from)?;

            let client = client_dao
                .lookup(&client_id)
                .await?
                .ok_or(ApiError::NotFound)?;

            Ok(Json(ClientResponse {
                client_id: client.client_id,
                client_name: client.client_name,
                scopes: client.scopes,
                grants: client.grants,
            }))
        })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateClientRequest {
    client_name: String,
    scopes: HashSet<Scope>,
    grants: HashSet<GrantType>,
    loopback: Option<bool>,
}

#[patch("/api/v1/client/<client_id>", data = "<form>")]
async fn update(
    client_id: String,
    authenticated: Authenticated,
    client_dao: State<'_, Arc<dyn ClientDao>>,
    form: Json<CreateClientRequest>,
) -> Result<Status, ApiError> {
    UPDATE_MEASURE
        .stats(async move {
            let request = form.into_inner();
            policy::client::update(&authenticated.claims).map_err(ApiError::from)?;

            client_dao
                .update(
                    &client_id,
                    request.client_name,
                    request.scopes,
                    request.grants,
                    request.loopback.unwrap_or(false),
                )
                .await?;

            Ok(Status::NoContent)
        })
        .await
        .map_err(ApiError::into)
}

pub(crate) fn routes() -> Vec<Route> {
    routes![register, get, update]
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::Arc;

    use chrono::Duration;
    use rocket::http::{ContentType, Header};

    use jwt::Issuer;

    use crate::service::token::TokenService;

    use super::*;

    fn create_request() -> CreateClientRequest {
        CreateClientRequest {
            client_name: "test_client".to_string(),
            scopes: [Scope::Superuser].iter().cloned().collect(),
            grants: [GrantType::Password].iter().cloned().collect(),
            loopback: None,
            credential: None,
        }
    }

    async fn setup() -> Result<
        (
            rocket::local::asynchronous::Client,
            Issuer,
            Arc<dyn ClientDao>,
        ),
        Box<dyn Error>,
    > {
        let rand = Arc::new(ring::rand::SystemRandom::new());
        let token = Arc::new(TokenService::new(rand.clone()));
        let issuer = Issuer::test(rand)?;
        let validator = issuer.new_validator()?;
        let dao = Arc::new(crate::dao::ClientDaoMemory::new(token));

        let rocket = rocket::ignite()
            .manage(validator)
            .manage(dao.clone() as Arc<dyn ClientDao>)
            .mount("/", routes());

        let client = rocket::local::asynchronous::Client::untracked(rocket)
            .await
            .expect("valid rocket instance");

        Ok((client, issuer, dao))
    }

    #[tokio::test]
    async fn test_unauthorized() -> Result<(), Box<dyn Error>> {
        let (client, _, _) = setup().await?;

        let request = create_request();

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .post("/api/v1/client")
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Unauthorized);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_no_credential() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;

        let token = issuer.issue(
            Some("test".to_string()),
            "foo".to_string(),
            [Scope::Superuser].iter(),
            Duration::seconds(60),
        )?;

        let request = create_request();

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .post("/api/v1/client")
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Ok);

        let body = res.into_bytes().await.unwrap();
        let decoded: CreateClientResponse =
            serde_json::from_slice(&body).expect("failed to deserialize response");

        let stored = dao
            .lookup(&decoded.client_id)
            .await?
            .expect("Not persisted");
        assert_eq!(decoded.client_credential, None);
        assert_eq!(stored.scopes, request.scopes);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_invalid_credential() -> Result<(), Box<dyn Error>> {
        let (client, issuer, _) = setup().await?;

        let token = issuer.issue::<Scope, _>(
            Some("test".to_string()),
            "foo".to_string(),
            std::iter::empty(),
            Duration::seconds(60),
        )?;

        let request = create_request();

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .post("/api/v1/client")
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Forbidden);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_client() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;

        let token = issuer.issue(
            Some("test".to_string()),
            "foo".to_string(),
            [Scope::Superuser].iter(),
            Duration::seconds(60),
        )?;

        let client_name = "test_client".to_string();
        let scopes: HashSet<_> = [Scope::Superuser].iter().cloned().collect();
        let grants: HashSet<_> = [GrantType::Password].iter().cloned().collect();

        let (client_id, _) = dao
            .register(
                client_name.clone(),
                scopes.clone(),
                grants.clone(),
                false,
                false,
                None,
            )
            .await?;

        let res = client
            .get(format!("/api/v1/client/{}", client_id))
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Ok);

        let body = res.into_bytes().await.unwrap();
        let decoded: ClientResponse =
            serde_json::from_slice(&body).expect("failed to deserialize response");

        assert_eq!(decoded.client_id, client_id);
        assert_eq!(decoded.client_name, client_name);
        assert_eq!(decoded.scopes, scopes);
        assert_eq!(decoded.grants, grants);

        Ok(())
    }

    #[tokio::test]
    async fn test_update() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;

        let token = issuer.issue(
            Some("test".to_string()),
            "foo".to_string(),
            [Scope::Superuser].iter(),
            Duration::seconds(60),
        )?;

        let client_name = "test_client".to_string();
        let client_new_name = "test_client2".to_string();
        let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let scopes_new: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let grants: HashSet<_> = Default::default();
        let grants_new: HashSet<_> = [GrantType::Password].iter().cloned().collect();

        let (client_id, _) = dao
            .register(client_name, scopes, grants, false, false, None)
            .await?;

        let request = UpdateClientRequest {
            client_name: client_new_name.clone(),
            scopes: scopes_new.clone(),
            grants: grants_new.clone(),
            loopback: None,
        };

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .patch(format!("/api/v1/client/{}", client_id))
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::NoContent);

        let stored = dao.lookup(&client_id).await?.expect("Not persisted");

        assert_eq!(stored.client_id, client_id);
        assert_eq!(stored.client_name, client_new_name);
        assert_eq!(stored.scopes, scopes_new);
        assert_eq!(stored.grants, grants_new);

        Ok(())
    }
}
