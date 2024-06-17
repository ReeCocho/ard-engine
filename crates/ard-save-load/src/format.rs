use serde::{de::DeserializeOwned, Serialize};

pub trait SaveFormat: Send + Sync {
    fn serialize<T: Serialize>(object: &T) -> Vec<u8>;

    fn deserialize<T: DeserializeOwned>(data: Vec<u8>) -> T;
}

pub struct Ron;

pub struct Bincode;

impl SaveFormat for Ron {
    fn serialize<T: Serialize>(object: &T) -> Vec<u8> {
        ron::ser::to_string(object).unwrap().into()
    }

    fn deserialize<T: DeserializeOwned>(data: Vec<u8>) -> T {
        ron::de::from_bytes(&data).unwrap()
    }
}

impl SaveFormat for Bincode {
    fn serialize<T: Serialize>(object: &T) -> Vec<u8> {
        bincode::serialize(object).unwrap()
    }

    fn deserialize<T: DeserializeOwned>(data: Vec<u8>) -> T {
        bincode::deserialize(&data).unwrap()
    }
}
