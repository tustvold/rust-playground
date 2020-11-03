use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use rusoto_dynamodb::AttributeValue;
use serde::{Deserialize, Serialize};

use dynamo_util::IntoAttribute;
use jwt::tag;

use crate::model::{ModelError, Scope};

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub user_id: String,
    pub full_name: String,
}

impl User {
    pub fn pk(user_id: &str) -> String {
        ["U", user_id].join("#")
    }
}

impl Into<HashMap<String, AttributeValue>> for User {
    fn into(self) -> HashMap<String, AttributeValue> {
        let mut map = HashMap::with_capacity(3);
        map.insert(String::from("pk"), Self::pk(&self.user_id).into_attribute());
        map.insert(String::from("full_name"), self.full_name.into_attribute());
        map
    }
}

impl TryFrom<HashMap<String, AttributeValue>> for User {
    type Error = ModelError;

    fn try_from(value: HashMap<String, AttributeValue>) -> Result<Self, Self::Error> {
        let mut pk = None;
        let mut full_name = None;

        for (key, v) in value.into_iter() {
            match key.as_str() {
                "pk" => pk = v.s,
                "full_name" => full_name = v.s,
                _ => {}
            }
        }

        let mut split = pk.as_ref().ok_or(ModelError::PrimaryKey)?.splitn(2, '#');
        let prefix = split.next().ok_or(ModelError::PrimaryKey)?;
        let user_id = split.next().ok_or(ModelError::PrimaryKey)?;

        if prefix != "U" {
            Err(ModelError::PrimaryKey)
        } else {
            Ok(Self {
                user_id: user_id.to_string(),
                full_name: full_name.ok_or(ModelError::MissingAttribute)?,
            })
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserCredential {
    pub username: String,
    pub user_id: String,
    pub credential: Vec<u8>,
    pub scopes: HashSet<Scope>,
}

impl UserCredential {
    pub fn pk(username: &str) -> String {
        ["UC", username].join("#")
    }
}

impl Into<HashMap<String, AttributeValue>> for UserCredential {
    fn into(self) -> HashMap<String, AttributeValue> {
        let mut map = HashMap::with_capacity(4);
        map.insert(
            String::from("pk"),
            Self::pk(&self.username).into_attribute(),
        );
        map.insert(String::from("credential"), self.credential.into_attribute());
        map.insert(String::from("user_id"), self.user_id.into_attribute());
        if !self.scopes.is_empty() {
            map.insert(String::from("scopes"), self.scopes.into_attribute());
        }
        map
    }
}

impl TryFrom<HashMap<String, AttributeValue>> for UserCredential {
    type Error = ModelError;

    fn try_from(value: HashMap<String, AttributeValue>) -> Result<Self, Self::Error> {
        let mut pk = None;
        let mut user_id = None;
        let mut credential = None;
        let mut scopes = None;

        for (key, v) in value.into_iter() {
            match key.as_str() {
                "pk" => pk = v.s,
                "user_id" => user_id = v.s,
                "credential" => credential = v.b,
                "scopes" => scopes = v.ss,
                _ => {}
            }
        }

        let mut split = pk.as_ref().ok_or(ModelError::PrimaryKey)?.splitn(2, '#');
        let prefix = split.next().ok_or(ModelError::PrimaryKey)?;
        let username = split.next().ok_or(ModelError::PrimaryKey)?;

        let scopes = scopes
            .map(|x| tag::parse_multiple(x.iter()))
            .transpose()
            .map_err(|e: strum::ParseError| ModelError::DeserializeError(e.to_string()))?
            .unwrap_or_else(Default::default);

        if prefix != "UC" {
            Err(ModelError::PrimaryKey)
        } else {
            Ok(Self {
                username: username.to_string(),
                user_id: user_id.ok_or(ModelError::MissingAttribute)?,
                credential: credential.ok_or(ModelError::MissingAttribute)?.to_vec(),
                scopes,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::*;

    #[test]
    fn test_user_credential_dynamo() -> Result<(), Box<dyn std::error::Error>> {
        let creds = vec![231, 55, 22, 45, 22];
        let val = UserCredential {
            username: "username_test".to_string(),
            user_id: "user_id_test".to_string(),
            credential: creds.clone(),
            scopes: [Scope::OfflineAccess].iter().cloned().collect(),
        };

        let map: HashMap<String, AttributeValue> = val.into();

        assert!(map.get("credential").unwrap().b.is_some());
        assert_eq!(
            map.get("user_id").unwrap().s,
            Some("user_id_test".to_string())
        );
        assert_eq!(
            map.get("pk").unwrap().s,
            Some("UC#username_test".to_string())
        );

        let back: UserCredential = map.try_into()?;

        assert_eq!("user_id_test", back.user_id);
        assert_eq!("username_test", back.username);
        assert_eq!(creds, back.credential);
        assert_eq!(back.scopes.len(), 1);
        assert!(back.scopes.contains(&Scope::OfflineAccess));

        Ok(())
    }

    #[test]
    fn test_user_dynamo() -> Result<(), Box<dyn std::error::Error>> {
        let val = User {
            user_id: "user_id_test".to_string(),
            full_name: "full_name".to_string(),
        };

        let map: HashMap<String, AttributeValue> = val.into();

        assert_eq!(
            map.get("pk").as_ref().unwrap().s,
            Some("U#user_id_test".to_string())
        );
        assert_eq!(
            map.get("full_name").as_ref().unwrap().s,
            Some("full_name".to_string())
        );

        let back: User = map.try_into()?;

        assert_eq!("user_id_test", back.user_id);
        assert_eq!("full_name", back.full_name);

        Ok(())
    }
}
