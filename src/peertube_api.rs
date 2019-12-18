use core::fmt;
use isahc::prelude::*;
use isahc::ResponseExt;
use log::info;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Serialize, Deserialize, Debug, Hash, Clone)]
pub struct Avatar {
    pub path: String,
    #[serde(rename(serialize = "createdAt", deserialize = "createdAt"))]
    pub created_at: String,
    #[serde(rename(serialize = "updatedAt", deserialize = "updatedAt"))]
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone)]
pub struct Instance {
    pub id: Option<u64>,
    pub uuid: String,
    pub url: String,
    pub name: String,
    #[serde(rename(serialize = "followingCount", deserialize = "followingCount"))]
    pub following_count: u64,
    #[serde(rename(serialize = "followersCount", deserialize = "followersCount"))]
    pub followers_count: u64,
    #[serde(rename(serialize = "createdAt", deserialize = "createdAt"))]
    pub created_at: String,
    #[serde(rename(serialize = "updatedAt", deserialize = "updatedAt"))]
    pub updated_at: String,
    pub avatar: Option<Avatar>,
}
#[derive(Serialize, Deserialize, Debug, Hash, Clone)]
pub struct Video {
    //#[elastic(id(expr = "id.to_string()"))]
    pub id: Option<i64>, // Should be i64
    pub uuid: String,
    #[serde(rename(serialize = "createdAt", deserialize = "createdAt"))]
    pub created_at: String,
    #[serde(rename(serialize = "publishedAt", deserialize = "publishedAt"))]
    pub published_at: String,
    #[serde(rename(serialize = "updatedAt", deserialize = "updatedAt"))]
    pub updated_at: String,
    #[serde(rename(
        serialize = "originallyPublishedAt",
        deserialize = "originallyPublishedAt"
    ))]
    pub originally_published_at: Option<String>,
    pub category: Category,
    pub licence: Licence,
    pub language: Language,
    pub privacy: Privacy,
    pub description: Option<String>,
    pub duration: i64, /* Should be u64 */
    #[serde(rename(serialize = "isLocal", deserialize = "isLocal"))]
    pub is_local: bool,
    #[serde(rename(serialize = "thumbnailPath", deserialize = "thumbnailPath"))]
    pub thumbnail_path: String,
    #[serde(rename(serialize = "previewPath", deserialize = "previewPath"))]
    pub preview_path: String,
    #[serde(rename(serialize = "embedPath", deserialize = "embedPath"))]
    pub embed_path: String,
    pub views: i64,    /* Should be u64 */
    pub likes: i64,    /* Should be u64 */
    pub dislikes: i64, /* Should be i64 */
    pub nsfw: bool,
    #[serde(rename(serialize = "waitTranscoding", deserialize = "waitTranscoding"))]
    pub wait_transcoding: Option<bool>,
    pub state: Option<State>,
    pub blacklisted: Option<bool>,
    #[serde(rename(serialize = "blacklistedReason", deserialize = "blacklistedReason"))]
    pub blacklisted_reason: Option<String>,
    pub account: Account,
    pub channel: Channel,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone)]
pub struct Account {
    pub id: Option<i64>,
    pub name: String,
    #[serde(rename(serialize = "displayName", deserialize = "displayName"))]
    pub display_name: String,
    pub url: String,
    pub host: String,
    pub avatar: Option<Avatar>,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone)]
pub struct Channel {
    pub id: Option<i64>,
    pub name: String,
    #[serde(rename(serialize = "displayName", deserialize = "displayName"))]
    pub display_name: String,
    pub url: String,
    pub host: String,
    pub avatar: Option<Avatar>,
}

macro_rules! peertube_field {
    ($name:ident, $id_type:ident) => {
        #[derive(Serialize, Deserialize, Debug, Hash, Clone)]
        pub struct $name {
            pub id: Option<$id_type>,
            pub label: String,
        }
    };
}

peertube_field!(Category, i64);
peertube_field!(Language, String);
peertube_field!(Privacy, i64);
peertube_field!(Licence, i64);
peertube_field!(State, i64);

#[derive(Debug)]
struct JoinPeertubeError(&'static str);

impl fmt::Display for JoinPeertubeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for JoinPeertubeError {}

pub fn fetch_instance_list_from_joinpeertube() -> Result<Vec<String>, Box<dyn Error>> {
    let json =
        Request::get("https://instances.joinpeertube.org/api/v1/instances?start=0&count=100000000")
            .body(())?
            .send()?
            .json::<serde_json::Value>()?;
    if let Some(data) = json["data"].as_array() {
        let mut result = vec![];
        for value in data {
            let mut host = value["host"].to_string();
            host.pop();
            host.remove(0);
            result.push(host);
        }
        info!("Added {} instances to the instance queue", result.len());
        Ok(result)
    } else {
        Err(Box::new(JoinPeertubeError(
            "https://instances.joinpeertube.org replied with invalid json",
        )))
    }
}

#[cfg(test)]
mod test {
    use crate::peertube_api::Video;

    #[test]
    fn peertube_api() {
        let json = include_str!("../tests/video1.json");
        let video: Video = serde_json::from_str(json).unwrap();
        println!("{:?}", video);
    }
}
