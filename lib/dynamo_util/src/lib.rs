use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rusoto_core::credential::StaticProvider;
use rusoto_dynamodb::{AttributeValue, DynamoDbClient, UpdateItemInput};
use rusoto_util::{parse_region, CustomChainProvider};

pub trait IntoAttribute {
    fn into_attribute(self) -> AttributeValue;
}

impl IntoAttribute for String {
    fn into_attribute(self) -> AttributeValue {
        AttributeValue {
            s: Some(self),
            ..Default::default()
        }
    }
}

impl IntoAttribute for bool {
    fn into_attribute(self) -> AttributeValue {
        AttributeValue {
            bool: Some(self),
            ..Default::default()
        }
    }
}

impl IntoAttribute for Vec<u8> {
    fn into_attribute(self) -> AttributeValue {
        AttributeValue {
            b: Some(self.into()),
            ..Default::default()
        }
    }
}

impl IntoAttribute for DateTime<Utc> {
    fn into_attribute(self) -> AttributeValue {
        AttributeValue {
            n: Some(self.timestamp().to_string()),
            ..Default::default()
        }
    }
}

impl<T: AsRef<str>> IntoAttribute for HashSet<T> {
    fn into_attribute(self) -> AttributeValue {
        AttributeValue {
            ss: Some(
                self.iter()
                    .map(|x| {
                        let s: &str = x.as_ref();
                        s.to_string()
                    })
                    .collect(),
            ),
            ..Default::default()
        }
    }
}

pub struct UpdateBuilder {
    set: Vec<String>,
    remove: Vec<String>,
    values: HashMap<String, AttributeValue>,
}

impl UpdateBuilder {
    pub fn new(capacity: usize) -> UpdateBuilder {
        UpdateBuilder {
            set: Vec::with_capacity(capacity),
            remove: Vec::with_capacity(capacity),
            values: HashMap::with_capacity(capacity),
        }
    }

    pub fn value<T: IntoAttribute>(mut self, key: &str, value: T) -> Self {
        self.set.push([key, " = :", key].concat());
        self.values
            .insert([":", key].concat(), value.into_attribute());
        self
    }

    pub fn remove(mut self, key: &str) -> Self {
        self.remove.push(key.to_string());
        self
    }

    pub fn build(
        self,
        key: HashMap<String, AttributeValue>,
        table_name: String,
    ) -> UpdateItemInput {
        let mut builder = Vec::with_capacity(2);
        if !self.set.is_empty() {
            builder.push(["SET ", &self.set.join(", ")].concat());
        }

        if !self.remove.is_empty() {
            builder.push(["REMOVE ", &self.remove.join(", ")].concat());
        }

        UpdateItemInput {
            key,
            table_name,
            update_expression: Some(builder.join(" ")),
            expression_attribute_values: Some(self.values),
            ..Default::default()
        }
    }
}

pub fn dynamo_client(region: String, endpoint: Option<String>, local: bool) -> DynamoDbClient {
    let region = parse_region(region, endpoint);
    let dispatcher =
        rusoto_core::request::HttpClient::new().expect("failed to create request dispatcher");

    if local {
        return DynamoDbClient::new_with(
            dispatcher,
            StaticProvider::new_minimal("local".to_string(), "development".to_string()),
            region,
        );
    }

    DynamoDbClient::new_with(dispatcher, CustomChainProvider::new(), region)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_update_builder_full() {
        let output = UpdateBuilder::new(5)
            .value("foo", "Hello World".to_string())
            .value("sdf", "sdd World".to_string())
            .remove("sdfs")
            .build(Default::default(), "foo".to_string());

        let vals = output.expression_attribute_values.as_ref().unwrap();

        assert_eq!(output.table_name, "foo");
        assert_eq!(
            output.update_expression.unwrap(),
            "SET foo = :foo, sdf = :sdf REMOVE sdfs"
        );

        assert_eq!(vals.len(), 2);
        assert!(vals.contains_key(":foo"));
        assert!(vals.contains_key(":sdf"));
        assert_eq!(vals[":foo"].s.as_ref().unwrap(), "Hello World");
        assert_eq!(vals[":sdf"].s.as_ref().unwrap(), "sdd World");
    }

    #[test]
    fn test_update_builder_set() {
        let output = UpdateBuilder::new(5)
            .value("foo", "Hello World".to_string())
            .value("sdf", "sdd World".to_string())
            .build(Default::default(), "foo".to_string());

        assert_eq!(
            output.update_expression.unwrap(),
            "SET foo = :foo, sdf = :sdf"
        );
    }

    #[test]
    fn test_update_builder_remove() {
        let output = UpdateBuilder::new(5)
            .remove("foo")
            .build(Default::default(), "foo".to_string());

        assert_eq!(output.update_expression.unwrap(), "REMOVE foo");
    }
}
