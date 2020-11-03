use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use chrono::{DateTime, TimeZone, Utc};
use rusoto_dynamodb::AttributeValue;
use serde::{Deserialize, Serialize};

use dynamo_util::IntoAttribute;
use jwt::tag;

use crate::model::{ModelError, Scope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenewalToken {
    pub client_id: String,
    pub subject: String,
    pub device_name: String,
    pub hashed_token: Vec<u8>,
    pub scopes: HashSet<Scope>,
    pub expiry: DateTime<Utc>,
}

impl RenewalToken {
    pub fn pk(client_id: &str, hashed_token: &[u8]) -> String {
        let encoded = base64::encode_config(hashed_token, base64::URL_SAFE_NO_PAD);
        ["RT", client_id, &encoded].join("#")
    }
}

impl Into<HashMap<String, AttributeValue>> for RenewalToken {
    fn into(self) -> HashMap<String, AttributeValue> {
        let mut map = HashMap::with_capacity(5);
        map.insert(
            String::from("pk"),
            Self::pk(&self.client_id, &self.hashed_token).into_attribute(),
        );
        map.insert(String::from("subject"), self.subject.into_attribute());
        map.insert(
            String::from("device_name"),
            self.device_name.into_attribute(),
        );
        if !self.scopes.is_empty() {
            map.insert(String::from("scopes"), self.scopes.into_attribute());
        }
        map.insert(String::from("expiry"), self.expiry.into_attribute());
        map
    }
}

impl TryFrom<HashMap<String, AttributeValue>> for RenewalToken {
    type Error = ModelError;

    fn try_from(value: HashMap<String, AttributeValue>) -> Result<Self, Self::Error> {
        let mut pk = None;
        let mut subject = None;
        let mut device_name = None;
        let mut scopes = None;
        let mut expiry = None;

        for (key, v) in value.into_iter() {
            match key.as_str() {
                "pk" => pk = v.s,
                "subject" => subject = v.s,
                "device_name" => device_name = v.s,
                "expiry" => expiry = v.n,
                "scopes" => scopes = v.ss,
                _ => {}
            }
        }

        let scopes = scopes
            .map(|x| tag::parse_multiple(x.iter()))
            .transpose()
            .map_err(|e: strum::ParseError| ModelError::DeserializeError(e.to_string()))?
            .unwrap_or_else(Default::default);

        let mut split = pk.as_ref().ok_or(ModelError::PrimaryKey)?.splitn(3, '#');
        let prefix = split.next().ok_or(ModelError::PrimaryKey)?;
        let client_id = split.next().ok_or(ModelError::PrimaryKey)?;
        let encoded_token = split.next().ok_or(ModelError::PrimaryKey)?;

        let hashed_token = base64::decode_config(&encoded_token, base64::URL_SAFE_NO_PAD)
            .map_err(|e| ModelError::DeserializeError(e.to_string()))?;

        let expiry = expiry
            .ok_or(ModelError::MissingAttribute)?
            .parse::<i64>()
            .map_err(|e| ModelError::DeserializeError(e.to_string()))?;

        if prefix != "RT" {
            Err(ModelError::PrimaryKey)
        } else {
            Ok(Self {
                client_id: client_id.to_string(),
                subject: subject.ok_or(ModelError::MissingAttribute)?,
                device_name: device_name.ok_or(ModelError::MissingAttribute)?,
                hashed_token,
                scopes,
                expiry: Utc.timestamp(expiry, 0),
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
        let val = RenewalToken {
            client_id: "cli".to_string(),
            subject: "sub".to_string(),
            device_name: "device_test".to_string(),
            hashed_token: vec![132, 55, 22],
            scopes: [Scope::OfflineAccess].iter().cloned().collect(),
            expiry: chrono::Utc::now(),
        };

        let map: HashMap<String, AttributeValue> = val.clone().into();

        let pk = map.get("pk").as_ref().unwrap().s.as_ref().unwrap();
        let subject = map.get("subject").as_ref().unwrap().s.as_ref().unwrap();
        let device_name = map.get("device_name").as_ref().unwrap().s.as_ref().unwrap();
        let scopes = map.get("scopes").as_ref().unwrap().ss.as_ref().unwrap();
        let expiry = map.get("expiry").as_ref().unwrap().n.as_ref().unwrap();

        let expected_pk = format!(
            "RT#{}#{}",
            val.client_id,
            base64::encode_config(&val.hashed_token, base64::URL_SAFE_NO_PAD)
        );

        assert_eq!(pk, &expected_pk);
        assert_eq!(subject, &val.subject);
        assert_eq!(device_name, &val.device_name);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], "offline_access");
        assert_eq!(expiry.parse::<i64>()?, val.expiry.timestamp());

        let back: RenewalToken = map.try_into()?;

        assert_eq!(back.client_id, val.client_id);
        assert_eq!(back.subject, val.subject);
        assert_eq!(back.device_name, val.device_name);
        assert_eq!(back.hashed_token, val.hashed_token);
        assert_eq!(back.scopes, val.scopes);
        assert_eq!(back.expiry.timestamp(), val.expiry.timestamp());

        Ok(())
    }

    #[test]
    fn test_empty() -> Result<(), Box<dyn std::error::Error>> {
        let val = RenewalToken {
            client_id: "cli".to_string(),
            subject: "sub".to_string(),
            device_name: "device_test".to_string(),
            hashed_token: vec![132, 55, 22],
            scopes: Default::default(),
            expiry: chrono::Utc::now(),
        };

        let map: HashMap<String, AttributeValue> = val.into();
        assert!(!map.contains_key("scopes"));
        Ok(())
    }
}
