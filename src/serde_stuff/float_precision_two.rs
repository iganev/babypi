use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S>(float: &f32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let float_str = format!("{:.2}", float);

    let parsed = float_str.parse::<f32>().unwrap();

    serializer.serialize_f32(parsed)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    f32::deserialize(deserializer)
}
