use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use rusoto_dynamodb::AttributeValue;
use serde::{Deserialize, Serialize};

use dynamo_util::IntoAttribute;
use jwt::tag;

use crate::model::{GrantType, ModelError, Scope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub client_id: String,
    pub client_name: String,
    pub credential: Option<Vec<u8>>,
    pub scopes: HashSet<Scope>,
    pub grants: HashSet<GrantType>,
    pub loopback: bool,
}

impl Client {
    pub fn pk(client_id: &str) -> String {
        ["C", client_id].join("#")
    }
}

impl Into<HashMap<String, AttributeValue>> for Client {
    fn into(self) -> HashMap<String, AttributeValue> {
        let mut map = HashMap::with_capacity(6);
        map.insert(
            String::from("pk"),
            Self::pk(&self.client_id).into_attribute(),
        );
        map.insert(
            String::from("client_name"),
            self.client_name.into_attribute(),
        );
        if let Some(credential) = self.credential {
            map.insert(String::from("credential"), credential.into_attribute());
        }
        if !self.scopes.is_empty() {
            map.insert(String::from("scopes"), self.scopes.into_attribute());
        }
        if !self.grants.is_empty() {
            map.insert(String::from("grants"), self.grants.into_attribute());
        }
        map.insert(
            String::from("loopback"),
            AttributeValue {
                bool: Some(self.loopback),
                ..Default::default()
            },
        );
        map
    }
}

impl TryFrom<HashMap<String, AttributeValue>> for Client {
    type Error = ModelError;

    fn try_from(value: HashMap<String, AttributeValue>) -> Result<Self, Self::Error> {
        let mut pk = None;
        let mut client_name = None;
        let mut credential = None;
        let mut scopes = None;
        let mut grants = None;
        let mut loopback = None;

        for (key, v) in value.into_iter() {
            match key.as_str() {
                "pk" => pk = v.s,
                "client_name" => client_name = v.s,
                "credential" => credential = v.b,
                "loopback" => loopback = v.bool,
                "scopes" => scopes = v.ss,
                "grants" => grants = v.ss,
                _ => {}
            }
        }

        let scopes = scopes
            .map(|x| tag::parse_multiple(x.iter()))
            .transpose()
            .map_err(|e: strum::ParseError| ModelError::DeserializeError(e.to_string()))?
            .unwrap_or_else(Default::default);

        let grants = grants
            .map(|x| tag::parse_multiple(x.iter()))
            .transpose()
            .map_err(|e: strum::ParseError| ModelError::DeserializeError(e.to_string()))?
            .unwrap_or_else(Default::default);

        let mut split = pk.as_ref().ok_or(ModelError::PrimaryKey)?.splitn(3, '#');
        let prefix = split.next().ok_or(ModelError::PrimaryKey)?;
        let client_id = split.next().ok_or(ModelError::PrimaryKey)?;

        if prefix != "C" {
            Err(ModelError::PrimaryKey)
        } else {
            Ok(Self {
                client_id: client_id.to_string(),
                client_name: client_name.ok_or(ModelError::MissingAttribute)?,
                credential: credential.map(|x| x.to_vec()),
                scopes,
                grants,
                loopback: loopback.ok_or(ModelError::MissingAttribute)?,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::*;

    #[test]
    fn test_encode_decode() -> Result<(), Box<dyn std::error::Error>> {
        let expected_cred = vec![23, 65, 22];
        let val = Client {
            client_id: "cli".to_string(),
            client_name: "name".to_string(),
            credential: Some(expected_cred.clone()),
            scopes: [Scope::OfflineAccess].iter().cloned().collect(),
            grants: [GrantType::ClientCredentials].iter().cloned().collect(),
            loopback: false,
        };

        let map: HashMap<String, AttributeValue> = val.clone().into();

        let pk = map.get("pk").as_ref().unwrap().s.as_ref().unwrap();
        let client_name = map.get("client_name").as_ref().unwrap().s.as_ref().unwrap();
        let credential = map.get("credential").as_ref().unwrap().b.as_ref().unwrap();
        let scopes = map.get("scopes").as_ref().unwrap().ss.as_ref().unwrap();
        let grants = map.get("grants").as_ref().unwrap().ss.as_ref().unwrap();
        let loopback = map.get("loopback").as_ref().unwrap().bool.unwrap();

        let expected_pk = format!("C#{}", val.client_id);

        assert_eq!(pk, &expected_pk);
        assert_eq!(client_name, &val.client_name);
        assert_eq!(credential, &expected_cred);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], "offline_access");
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0], "client_credentials");
        assert_eq!(loopback, val.loopback);

        let back: Client = map.try_into()?;

        assert_eq!(back.client_id, val.client_id);
        assert_eq!(back.client_name, val.client_name);
        assert_eq!(back.credential, val.credential);
        assert_eq!(back.scopes, val.scopes);
        assert_eq!(back.grants, val.grants);
        assert_eq!(back.loopback, val.loopback);

        Ok(())
    }

    #[test]
    fn test_empty() -> Result<(), Box<dyn std::error::Error>> {
        let val = Client {
            client_id: "cli".to_string(),
            client_name: "name".to_string(),
            credential: None,
            scopes: Default::default(),
            grants: Default::default(),
            loopback: false,
        };

        let map: HashMap<String, AttributeValue> = val.into();

        assert!(!map.contains_key("credential"));
        assert!(!map.contains_key("scopes"));
        assert!(!map.contains_key("grants"));

        let _: Client = map.try_into()?;

        Ok(())
    }
}
