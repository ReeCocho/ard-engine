use std::error::Error;

use serde::{de::DeserializeOwned, Serialize};

pub trait SaveFormat: Send + Sync + 'static {
    type SerializeError: Error;
    type DeserializeError: Error;

    fn serialize<T: Serialize>(object: &T) -> Result<Vec<u8>, Self::SerializeError>;

    fn deserialize<T: DeserializeOwned>(data: Vec<u8>) -> Result<T, Self::DeserializeError>;
}

pub struct Ron;

pub struct Bincode;

impl SaveFormat for Ron {
    type SerializeError = ron::Error;
    type DeserializeError = ron::error::SpannedError;

    fn serialize<T: Serialize>(object: &T) -> Result<Vec<u8>, Self::SerializeError> {
        Ok(ron::ser::to_string(object)?.into())
    }

    fn deserialize<T: DeserializeOwned>(data: Vec<u8>) -> Result<T, Self::DeserializeError> {
        ron::de::from_bytes(&data)
    }
}

impl SaveFormat for Bincode {
    type SerializeError = bincode::Error;
    type DeserializeError = bincode::Error;

    fn serialize<T: Serialize>(object: &T) -> Result<Vec<u8>, Self::SerializeError> {
        bincode::serialize(object)
    }

    fn deserialize<T: DeserializeOwned>(data: Vec<u8>) -> Result<T, Self::DeserializeError> {
        bincode::deserialize(&data)
    }
}
