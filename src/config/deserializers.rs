use std::time::Duration;
use serde::{Deserialize, Deserializer};
use serde::de::Error;

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    match humantime::parse_duration(&s) {
        Ok(d) => {
            return Ok(d)
        },
        Err(e) => {
            return Err(D::Error::custom("cannot deserialize duration"));
        }
    }
}
