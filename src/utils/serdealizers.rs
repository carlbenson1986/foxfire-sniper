use std::fmt::Debug;
use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{IsNull, Output, ToSql};
use diesel::{deserialize, serialize, sql_types, Insertable};
use diesel_derives::{AsExpression, FromSqlRow};
use futures_util::TryFutureExt;
use serde::{de::Error, Deserialize, Deserializer, Serialize};
use solana_sdk::bs58;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::io::Write;
use std::str::FromStr;
use diesel::sql_types::Jsonb;

#[derive(Debug, FromSqlRow, AsExpression)]
#[diesel(check_for_backend(Pg))]
#[diesel(sql_type = sql_types::Text)]
pub struct PubkeyString(pub Pubkey);

#[derive(Debug, FromSqlRow, AsExpression)]
#[diesel(check_for_backend(Pg))]
#[diesel(sql_type = sql_types::Text)]
pub struct SignatureString(pub Signature);

impl Into<PubkeyString> for Pubkey {
    fn into(self) -> PubkeyString {
        PubkeyString(self)
    }
}

impl TryFrom<PubkeyString> for Pubkey {
    type Error = std::io::Error;

    fn try_from(value: PubkeyString) -> Result<Self, Self::Error> {
        Ok(value.0)
    }
}

impl TryFrom<String> for PubkeyString {
    type Error = std::io::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(PubkeyString(Pubkey::from_str(&value).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?))
    }
}

impl ToSql<sql_types::Text, Pg> for PubkeyString {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        out.write_all(self.0.to_string().as_bytes())?;
        Ok(IsNull::No)
    }
}

impl FromSql<sql_types::Text, Pg> for PubkeyString {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        Ok(PubkeyString(
            bs58::decode(s)
                .into_vec()
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap(),
        ))
    }
}

impl Into<SignatureString> for Signature {
    fn into(self) -> SignatureString {
        SignatureString(self)
    }
}

impl ToSql<sql_types::Text, Pg> for SignatureString {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        out.write_all(self.0.to_string().as_bytes())?;
        Ok(IsNull::No)
    }
}

impl FromSql<sql_types::Text, Pg> for SignatureString {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        Ok(SignatureString(
            bs58::decode(s)
                .into_vec()
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap(),
        ))
    }
}

pub fn deserialize_pubkey<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Pubkey::from_str(&s).map_err(serde::de::Error::custom)
}

pub fn deserialize_pubkey_opt<'de, D>(deserializer: D) -> Result<Option<Pubkey>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    match s {
        Some(s) => Pubkey::from_str(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

pub fn serialize_pubkey<S>(key: &Pubkey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&key.to_string())
}

pub fn serialize_pubkey_opt<S>(key: &Option<Pubkey>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match key {
        Some(key) => serializer.serialize_str(&key.to_string()),
        None => serializer.serialize_none(),
    }
}


#[derive(Debug, Clone, FromSqlRow, AsExpression)]
#[diesel(sql_type = Jsonb)]
pub struct JsonbVec<T>(pub Vec<T>);

impl<T> ToSql<Jsonb, Pg> for JsonbVec<T>
where
    T: serde::Serialize + Debug,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(&self.0)?;
        let json_string = serde_json::to_string(&value)?;
        out.write_all(json_string.as_bytes())?;
        Ok(IsNull::No)
    }
}

impl<T> From<Vec<T>> for JsonbVec<T> {
    fn from(vec: Vec<T>) -> Self {
        JsonbVec(vec)
    }
}

#[derive(Debug, Clone, FromSqlRow, AsExpression)]
#[diesel(sql_type = Jsonb)]
pub struct JsonbWrapper<T>(pub T);

impl<T> From<T> for JsonbWrapper<T> {
    fn from(value: T) -> Self {
        JsonbWrapper(value)
    }
}

impl<T> ToSql<Jsonb, Pg> for JsonbWrapper<T>
where
    T: Serialize + Debug,
{

    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(&self.0)?;
        out.write_all(&[1])?; // Write JSONB version (always 1 for now)
        serde_json::to_writer(out, &value)?;
        Ok(IsNull::No)
    }
}
