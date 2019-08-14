use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Avatar {
    pub path: String,
    #[serde(rename(serialize = "createdAt", deserialize = "createdAt"))]
    pub created_at: String,
    #[serde(rename(serialize = "updatedAt", deserialize = "updatedAt"))]
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Instance {
    pub id: u64,
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
    pub avatar: Avatar,
}
#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Video {
    pub id: u64,
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
    pub originally_published_at: String,
    pub category: String,
    pub licence: String,
    pub language: String,
    pub privacy: String,
    pub description: String,
    pub duration: u64,
    pub is_local: bool,
    #[serde(rename(serialize = "thumbnailPath", deserialize = "thumbnailPath"))]
    pub thumbnail_path: String,
    #[serde(rename(serialize = "previewPath", deserialize = "previewPath"))]
    pub preview_path: String,
    #[serde(rename(serialize = "embedPath", deserialize = "embedPath"))]
    pub embed_path: String,
    pub views: u64,
    pub likes: u64,
    pub dislikes: u64,
    pub nsfw: bool,
    pub wait_transcoding: bool,
    pub state: String,
    pub blacklisted: bool,
    #[serde(rename(serialize = "blacklistedReason", deserialize = "blacklistedReason"))]
    pub blacklisted_reason: String,
    pub account: Account,
    pub channel: Channel,
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Account {
    pub id: u64,
    pub name: String,
    #[serde(rename(serialize = "displayName", deserialize = "displayName"))]
    pub display_name: String,
    pub url: String,
    pub host: String,
    pub avatar: Avatar,
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    #[serde(rename(serialize = "displayName", deserialize = "displayName"))]
    pub display_name: String,
    pub url: String,
    pub host: String,
    pub avatar: Avatar,
}
