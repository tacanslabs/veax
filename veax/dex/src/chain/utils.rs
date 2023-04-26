pub mod serde_as_str {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::fmt::Display;
    use std::str::FromStr;

    pub fn serialize<E: Serializer, T: ToString>(val: &T, enc: E) -> Result<E::Ok, E::Error> {
        enc.serialize_str(&val.to_string())
    }

    pub fn deserialize<'d, E: Deserializer<'d>, T: FromStr>(enc: E) -> Result<T, E::Error>
    where
        T::Err: Display,
    {
        T::from_str(&String::deserialize(enc)?).map_err(Error::custom)
    }
}
