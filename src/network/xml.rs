use quick_xml::DeError;
use strum::{Display};
use serde::{Deserialize, Serialize};
use quick_xml::se::to_string_with_root;
use quick_xml::de::from_str;

#[derive(Display, Deserialize, Serialize)]
pub enum MessageType {
    ChatPlain
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UserMessage<'a> {
    pub id: &'a str,
    pub m: &'a str,
    pub t: MessageType,
}

pub struct XML {}

impl XML {

  pub fn read<'de, T>(s: &'de str) -> Result<T, DeError>
  where T: Deserialize<'de>
  {
    from_str(s)
  }

  pub fn write<T: Serialize> (json: &T) -> Result<String, quick_xml::SeError> {
    to_string_with_root("msg", json)
  }
}

