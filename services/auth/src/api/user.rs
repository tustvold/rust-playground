use std::collections::HashSet;
use std::sync::Arc;

use rocket::http::Status;
use rocket::{Route, State};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};

use rocket_util::Authenticated;
use telemetry::Measure;

use crate::api::error::ApiError;
use crate::dao::UserDao;
use crate::model::{Scope, User};
use crate::policy;

lazy_static! {
    static ref REGISTER_MEASURE: Measure = Measure::new("controller", "user_register");
    static ref GET_MEASURE: Measure = Measure::new("controller", "user_get");
    static ref GET_USERNAME_MEASURE: Measure = Measure::new("controller", "username_get");
    static ref CHANGE_USERNAME_MEASURE: Measure = Measure::new("controller", "change_username");
    static ref CHANGE_PASSWORD_MEASURE: Measure = Measure::new("controller", "change_password");
    static ref CHANGE_SCOPES_MEASURE: Measure = Measure::new("controller", "change_scopes");
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    full_name: String,
}

#[post("/api/v1/register", data = "<data>")]
async fn register(
    user_dao: State<'_, Arc<dyn UserDao>>,
    data: Json<RegisterRequest>,
) -> Result<Status, ApiError> {
    REGISTER_MEASURE
        .stats(async move {
            let user_id = user_dao.create_user(&data.full_name, None).await?;

            user_dao
                .create_credential(&data.username, &user_id, &data.password, Default::default())
                .await?;

            Ok(Status::NoContent)
        })
        .await
}

#[get("/api/v1/user/<user_id>")]
async fn get_user(
    user_id: String,
    authenticated: Authenticated,
    user_dao: State<'_, Arc<dyn UserDao>>,
) -> Result<Json<User>, ApiError> {
    GET_MEASURE
        .stats(async move {
            policy::user::get(&user_id, &authenticated.claims).map_err(ApiError::from)?;

            let user = user_dao
                .get_user(&user_id)
                .await
                .map_err(ApiError::from)?
                .ok_or(ApiError::NotFound)?;

            Ok(Json(user))
        })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct UsernameResponse {
    user_id: String,
}

#[get("/api/v1/username/<username>")]
async fn get_username(
    username: String,
    authenticated: Authenticated,
    user_dao: State<'_, Arc<dyn UserDao>>,
) -> Result<Json<UsernameResponse>, ApiError> {
    GET_USERNAME_MEASURE
        .stats(async move {
            let credential = user_dao
                .get_credential(&username)
                .await
                .map_err(ApiError::from)?
                .ok_or(ApiError::NotFound)?;

            policy::user::get_username(&credential.user_id, &authenticated.claims)
                .map_err(ApiError::from)?;

            Ok(Json(UsernameResponse {
                user_id: credential.user_id,
            }))
        })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

#[patch("/api/v1/username/<username>/password", data = "<data>")]
async fn change_password(
    username: String,
    user_dao: State<'_, Arc<dyn UserDao>>,
    data: Json<ChangePasswordRequest>,
) -> Result<Status, ApiError> {
    CHANGE_PASSWORD_MEASURE
        .stats(async move {
            user_dao.verify(&username, &data.current_password).await?;

            user_dao
                .update_password(&username, &data.new_password)
                .await?;

            Ok(Status::NoContent)
        })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct ChangeUsername {
    new_username: String,
    current_password: String,
    new_password: String,
}

#[patch("/api/v1/username/<username>", data = "<data>")]
async fn change_username(
    username: String,
    user_dao: State<'_, Arc<dyn UserDao>>,
    data: Json<ChangeUsername>,
) -> Result<Status, ApiError> {
    CHANGE_USERNAME_MEASURE
        .stats(async move {
            let cred = user_dao.verify(&username, &data.current_password).await?;

            user_dao
                .create_credential(
                    &data.new_username,
                    &cred.user_id,
                    &data.new_password,
                    cred.scopes,
                )
                .await?;

            user_dao.delete_credential(&username).await?;

            Ok(Status::NoContent)
        })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct ChangeScopes {
    scopes: HashSet<Scope>,
}

#[patch("/api/v1/username/<username>/scopes", data = "<data>")]
async fn change_scopes(
    username: String,
    authenticated: Authenticated,
    user_dao: State<'_, Arc<dyn UserDao>>,
    data: Json<ChangeScopes>,
) -> Result<Status, ApiError> {
    CHANGE_SCOPES_MEASURE
        .stats(async move {
            policy::user::change_scopes(&authenticated.claims).map_err(ApiError::from)?;

            let request = data.into_inner();
            user_dao.update_scopes(&username, request.scopes).await?;

            Ok(Status::NoContent)
        })
        .await
}

pub fn routes() -> Vec<Route> {
    routes![
        register,
        get_user,
        get_username,
        change_password,
        change_username,
        change_scopes
    ]
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use chrono::Duration;
    use ring::rand::SystemRandom;
    use rocket::http::{ContentType, Header};

    use jwt::{Issuer, IssuerError};

    use crate::dao::{DaoError, UserDaoMemory};
    use crate::model::User;

    use super::*;

    async fn setup() -> Result<
        (
            rocket::local::asynchronous::Client,
            Issuer,
            Arc<dyn UserDao>,
        ),
        Box<dyn Error>,
    > {
        let rand = Arc::new(SystemRandom::new());
        let issuer = Issuer::test(rand)?;
        let validator = issuer.new_validator()?;
        let dao = Arc::new(UserDaoMemory::new());

        let rocket = rocket::ignite()
            .manage(issuer.clone())
            .manage(validator)
            .manage(dao.clone() as Arc<dyn UserDao>)
            .mount("/", routes());

        let client = rocket::local::asynchronous::Client::untracked(rocket)
            .await
            .expect("valid rocket instance");

        Ok((client, issuer, dao))
    }

    #[tokio::test]
    async fn test_register() -> Result<(), Box<dyn Error>> {
        let (client, _, dao) = setup().await?;

        let request = RegisterRequest {
            username: "test_user".to_string(),
            password: "password123".to_string(),
            full_name: "full_name_test".to_string(),
        };

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .post("/api/v1/register")
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::NoContent);

        let cred = dao.verify(&request.username, &request.password).await?;

        dao.get_user(&cred.user_id).await?.expect("not persisted");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_unauthorized() -> Result<(), Box<dyn Error>> {
        let (client, _, _) = setup().await?;

        let res = client.get("/api/v1/user/foo").dispatch().await;
        assert_eq!(res.status(), Status::Unauthorized);
        Ok(())
    }

    fn token(issuer: &Issuer) -> Result<String, IssuerError> {
        issuer.issue::<Scope, _>(
            Some("test_user_id".to_string()),
            "client".to_string(),
            std::iter::empty(),
            Duration::seconds(60),
        )
    }

    #[tokio::test]
    async fn test_get_different_user() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;

        let token = token(&issuer)?;
        dao.create_user("Foo", Some("foo".to_string())).await?;

        let res = client
            .get("/api/v1/user/foo")
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Forbidden);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_user() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;

        let token = token(&issuer)?;
        let full_name = "Foo";
        let user_id = dao
            .create_user(full_name, Some("test_user_id".to_string()))
            .await?;

        let res = client
            .get("/api/v1/user/test_user_id")
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Ok);

        let body = res.into_bytes().await.unwrap();
        let decoded: User = serde_json::from_slice(&body).expect("failed to deserialize response");

        assert_eq!(decoded.user_id, user_id);
        assert_eq!(&decoded.full_name, full_name);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_credential_unauthorized() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;
        let token = token(&issuer)?;
        let user_id = dao.create_user("Foo", Some("user_id".to_string())).await?;

        dao.create_credential("fizbuz", &user_id, "password123", Default::default())
            .await?;

        let res = client
            .get("/api/v1/username/fizbuz")
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Forbidden);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_credential() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;
        let token = token(&issuer)?;
        let user_id = dao
            .create_user("Foo", Some("test_user_id".to_string()))
            .await?;

        dao.create_credential("fizbuz", &user_id, "password123", Default::default())
            .await?;

        let res = client
            .get("/api/v1/username/fizbuz")
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Ok);

        let body = res.into_bytes().await.unwrap();
        let decoded: UsernameResponse =
            serde_json::from_slice(&body).expect("failed to deserialize response");

        assert_eq!(decoded.user_id, user_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_change_password() -> Result<(), Box<dyn Error>> {
        let (client, _, dao) = setup().await?;
        let user_id = dao
            .create_user("Foo", Some("test_user_id".to_string()))
            .await?;

        dao.create_credential("fizbuz", &user_id, "password123", Default::default())
            .await?;

        let request = ChangePasswordRequest {
            current_password: "password123".to_string(),
            new_password: "ashgdfg".to_string(),
        };

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .patch("/api/v1/username/fizbuz/password")
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::NoContent);

        dao.verify("fizbuz", &request.new_password).await?;

        match dao.verify("fizbuz", &request.current_password).await {
            Err(DaoError::InvalidCredential) => (),
            _ => panic!(),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_change_username() -> Result<(), Box<dyn Error>> {
        let (client, _, dao) = setup().await?;
        let user_id = dao
            .create_user("Foo", Some("test_user_id".to_string()))
            .await?;
        dao.create_credential("fizbuz", &user_id, "password123", Default::default())
            .await?;

        let request = ChangeUsername {
            new_username: "foobar".to_string(),
            current_password: "password123".to_string(),
            new_password: "ashgdfg".to_string(),
        };

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .patch("/api/v1/username/fizbuz")
            .header(ContentType::JSON)
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::NoContent);

        dao.verify(&request.new_username, &request.new_password)
            .await?;

        match dao.verify("fizbuz", &request.current_password).await {
            Err(DaoError::NotFound) => (),
            _ => panic!(),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_change_scopes_forbidden() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;
        let token = token(&issuer)?;
        let user_id = dao
            .create_user("Foo", Some("test_user_id".to_string()))
            .await?;

        dao.create_credential("fizbuz", &user_id, "password123", Default::default())
            .await?;

        let request = ChangeScopes {
            scopes: [Scope::Superuser].iter().cloned().collect(),
        };

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .patch("/api/v1/username/fizbuz/scopes")
            .header(ContentType::JSON)
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Forbidden);
        Ok(())
    }

    #[tokio::test]
    async fn test_change_scopes() -> Result<(), Box<dyn Error>> {
        let (client, issuer, dao) = setup().await?;
        let token = issuer.issue(
            Some("test_user_id".to_string()),
            "client".to_string(),
            [Scope::Superuser].iter(),
            Duration::seconds(60),
        )?;

        let user_id = dao
            .create_user("Foo", Some("test_user_id".to_string()))
            .await?;

        dao.create_credential("fizbuz", &user_id, "password123", Default::default())
            .await?;

        let request = ChangeScopes {
            scopes: [Scope::Superuser].iter().cloned().collect(),
        };

        let body = serde_json::to_string(&request).expect("request must serialize");
        let res = client
            .patch("/api/v1/username/fizbuz/scopes")
            .header(ContentType::JSON)
            .header(Header::new("Authorization", format!("bearer {}", token)))
            .body(body)
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::NoContent);

        let cred = dao.get_credential("fizbuz").await?.expect("not persisted");
        assert_eq!(cred.scopes, request.scopes);

        Ok(())
    }
}
