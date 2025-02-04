/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

// Locales supported by Merino Curated Reccomendations
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum Locale {
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "fr-FR")]
    FrFr,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "es-ES")]
    EsEs,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "it-IT")]
    ItIt,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "en-CA")]
    EnCa,
    #[serde(rename = "en-GB")]
    EnGb,
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "de-DE")]
    DeDe,
    #[serde(rename = "de-AT")]
    DeAt,
    #[serde(rename = "de-CH")]
    DeCh,
}
// Configuration settings for a Section
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SectionSettings {
    #[serde(rename = "sectionId")]
    section_id: String,
    #[serde(rename = "isFollowed")]
    is_followed: bool,
    #[serde(rename = "isBlocked")]
    is_blocked: bool,
}

// Information required to request curated recommendations
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CuratedRecommendationsRequest {
    pub locale: Locale,
    pub region: Option<String>,
    pub count: Option<i32>,
    pub topics: Option<Vec<String>>,
    pub feeds: Option<Vec<String>>,
    pub sections: Option<Vec<SectionSettings>>,
    #[serde(rename = "experimentName")]
    pub experiment_name: Option<String>,
    #[serde(rename = "experimentBranch")]
    pub experiment_branch: Option<String>,
}

// Response schema for a list of curated recommendations
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CuratedRecommendationsResponse {
    #[serde(rename = "recommendedAt")]
    pub recommended_at: i32,
    pub data: Vec<ReccomendationDataItem>,
    pub feeds: Option<Feeds>,
}

// Multiple list of curated recoummendations
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Feeds {
    pub need_to_know: Option<CuratedRecommendationsBucket>,
    pub fakespot: Option<FakespotFeed>,
    pub top_stories_section: Option<FeedSection>,
    pub business: Option<FeedSection>,
    pub career: Option<FeedSection>,
    pub arts: Option<FeedSection>,
    pub food: Option<FeedSection>,
    pub health: Option<FeedSection>,
    pub home: Option<FeedSection>,
    pub finance: Option<FeedSection>,
    pub government: Option<FeedSection>,
    pub sports: Option<FeedSection>,
    pub tech: Option<FeedSection>,
    pub travel: Option<FeedSection>,
    pub education: Option<FeedSection>,
    pub hobbies: Option<FeedSection>,
    #[serde(rename = "society-parenting")]
    pub society_parenting: Option<FeedSection>,
    #[serde(rename = "education-science")]
    pub education_science: Option<FeedSection>,
    pub society: Option<FeedSection>,
}

// Curated Recommendation Information
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ReccomendationDataItem {
    #[serde(rename = "corpusItemId")]
    pub corpus_item_id: Uuid,
    #[serde(rename = "scheduledCorpusItemId")]
    pub schdeuled_corpus_item_id: Uuid,
    pub url: Url,
    pub title: String,
    pub excerpt: String,
    pub topic: Option<String>,
    pub publisher: String,
    #[serde(rename = "isTimeSensitive")]
    pub is_time_sensitive: bool,
    #[serde(rename = "imageUrl")]
    pub image_url: Url,
    #[serde(rename = "tileId")]
    pub tile_id: i32,
    #[serde(rename = "receivedRank")]
    pub received_rank: i32,
}

// Ranked list of curated recommendations
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CuratedRecommendationsBucket {
    pub recommendations: Vec<ReccomendationDataItem>,
    pub title: Option<String>,
}

// Fakespot product reccomendations
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FakespotFeed {
    pub products: Vec<FakespotProduct>,
    #[serde(rename = "defaultCategoryName")]
    pub default_category_name: String,
    #[serde(rename = "headerCopy")]
    pub header_copy: String,
    #[serde(rename = "footerCopy")]
    pub footer_copy: String,
    pub cta: FakespotCta,
}

// Fakespot product details
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FakespotProduct {
    id: String,
    title: String,
    category: String,
    #[serde(rename = "imageUrl")]
    image_url: Url,
    url: Url,
}

// Fakespot CTA
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FakespotCta {
    #[serde(rename = "ctaCopy")]
    pub cta_copy: String,
    pub url: Url,
}

// Ranked list of curated recommendations with responsive layout configs
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FeedSection {
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    pub recommendations: Vec<ReccomendationDataItem>,
    pub title: String,
    pub subtitle: Option<String>,
    pub layout: Layout,
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

// Representation of a responsive layout configuration with multiple column layouts
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Layout {
    pub name: String,
    #[serde(rename = "responsiveLayouts")]
    pub responsive_layouts: Vec<ResponsiveLayout>,
}

// Layout configurations within a column
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ResponsiveLayout {
    #[serde(rename = "columnCount")]
    pub column_count: i32,
    pub tiles: Vec<Tile>,
}
// Properties for a single tile in a responsive layout
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Tile {
    pub size: String,
    pub position: i32,
    #[serde(rename = "hasAd")]
    pub has_ad: bool,
    #[serde(rename = "hasExcerpt")]
    pub has_excerpt: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_curated_reccomendations_request_deserialization() {
        let json_data = r#"
        {
            "locale": "en-US",
            "region": "North America",
            "count": 100,
            "topics": [
                "business"
            ],
            "feeds": [
                "need_to_know"
            ],
            "sections": [
                {
                    "sectionId": "sports",
                    "isFollowed": true,
                    "isBlocked": true
                }
            ],
            "experimentName": "new-tab-extend-content-duration",
            "experimentBranch": "control"
        }"#;

        let recommendation_request: CuratedRecommendationsRequest =
            serde_json::from_str(json_data).unwrap();
        assert_eq!(
            recommendation_request,
            CuratedRecommendationsRequest {
                locale: Locale::EnUs,
                region: Some("North America".to_string()),
                count: Some(100),
                topics: Some(vec!["business".to_string()]),
                feeds: Some(vec!["need_to_know".to_string()]),
                sections: Some(vec![SectionSettings {
                    section_id: "sports".to_string(),
                    is_blocked: true,
                    is_followed: true
                }]),
                experiment_name: Some("new-tab-extend-content-duration".to_string()),
                experiment_branch: Some("control".to_string())
            }
        );
    }
    #[test]
    fn test_fakespot_product_deserialization() {
        let json_data = r#"
        {
          "id": "fakespot",
          "title": "Fakespot Product",
          "category": "News",
          "imageUrl": "https://example.com",
          "url": "https://example.com"
        }"#;

        let fs_product: FakespotProduct = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            fs_product,
            FakespotProduct {
                id: "fakespot".to_string(),
                title: "Fakespot Product".to_string(),
                category: "News".to_string(),
                image_url: Url::parse("https://example.com").unwrap(),
                url: Url::parse("https://example.com").unwrap()
            }
        );
    }

    #[test]
    fn test_tile_deserialization() {
        let json_data = r#"
        {
          "size": "small",
          "position": 0,
          "hasAd": true,
          "hasExcerpt": true
        }"#;

        let tile: Tile = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            tile,
            Tile {
                size: "small".to_string(),
                position: 0,
                has_ad: true,
                has_excerpt: true
            }
        );
    }

    #[test]
    fn test_cta_deserialization() {
        let json_data = r#"
        {
          "ctaCopy": "Fakespot blurb",
          "url": "https://example.com"
        }"#;

        let cta: FakespotCta = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            cta,
            FakespotCta {
                cta_copy: "Fakespot blurb".to_string(),
                url: Url::parse("https://example.com").unwrap()
            }
        );
    }

    #[test]
    fn test_responsibe_layout_deserialization() {
        let json_data = r#"
        {
            "columnCount": 1,
            "tiles": [
              {
                "size": "small",
                "position": 0,
                "hasAd": true,
                "hasExcerpt": true
              }
            ]
        }"#;

        let rl: ResponsiveLayout = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            rl,
            ResponsiveLayout {
                column_count: 1,
                tiles: vec![Tile {
                    size: "small".to_string(),
                    position: 0,
                    has_ad: true,
                    has_excerpt: true
                }]
            }
        );
    }

    #[test]
    fn test_recommendation_data_item_deserialization() {
        let json_data = r#"
        {
          "corpusItemId": "17538d96-71dc-4196-bb2a-968cddc15474",
          "scheduledCorpusItemId": "5f72b12c-2723-470a-a51d-6fe88aa555d7",
          "url": "https://example.com/",
          "title": "Rec Title",
          "excerpt": "This is a blurb about the rec.",
          "topic": "business",
          "publisher": "moz",
          "isTimeSensitive": true,
          "imageUrl": "https://example.com/",
          "tileId": 10000000,
          "receivedRank": 0
        }"#;

        let rdi: ReccomendationDataItem = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            rdi,
            ReccomendationDataItem {
                corpus_item_id: Uuid::parse_str("17538d96-71dc-4196-bb2a-968cddc15474").unwrap(),
                schdeuled_corpus_item_id: Uuid::parse_str("5f72b12c-2723-470a-a51d-6fe88aa555d7")
                    .unwrap(),
                url: Url::parse("https://example.com").unwrap(),
                title: "Rec Title".to_string(),
                excerpt: "This is a blurb about the rec.".to_string(),
                topic: Some("business".to_string()),
                publisher: "moz".to_string(),
                is_time_sensitive: true,
                image_url: Url::parse("https://example.com").unwrap(),
                tile_id: 10000000,
                received_rank: 0
            }
        );
    }

    #[test]
    fn test_curated_reccomendations_bucket_deserialization() {
        let json_data = r#"
        {
          "recommendations":[
            {
              "corpusItemId": "17538d96-71dc-4196-bb2a-968cddc15474",
              "scheduledCorpusItemId": "5f72b12c-2723-470a-a51d-6fe88aa555d7",
              "url": "https://example.com/",
              "title": "Rec Title",
              "excerpt": "This is a blurb about the rec.",
              "topic": "business",
              "publisher": "moz",
              "isTimeSensitive": true,
              "imageUrl": "https://example.com/",
              "tileId": 10000000,
              "receivedRank": 0
            }
          ],
          "title": "recommendations_bucket_title"

        }"#;

        let crb: CuratedRecommendationsBucket = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            crb,
            CuratedRecommendationsBucket {
                recommendations: vec![ReccomendationDataItem {
                    corpus_item_id: Uuid::parse_str("17538d96-71dc-4196-bb2a-968cddc15474")
                        .unwrap(),
                    schdeuled_corpus_item_id: Uuid::parse_str(
                        "5f72b12c-2723-470a-a51d-6fe88aa555d7"
                    )
                    .unwrap(),
                    url: Url::parse("https://example.com").unwrap(),
                    title: "Rec Title".to_string(),
                    excerpt: "This is a blurb about the rec.".to_string(),
                    topic: Some("business".to_string()),
                    publisher: "moz".to_string(),
                    is_time_sensitive: true,
                    image_url: Url::parse("https://example.com").unwrap(),
                    tile_id: 10000000,
                    received_rank: 0
                }],
                title: Some("recommendations_bucket_title".to_string())
            },
        );
    }

    #[test]
    fn test_layout_deserialization() {
        let json_data = r#"
        {
          "name": "4-medium-small-1",
          "responsiveLayouts": [
           {
              "columnCount": 1,
              "tiles": [
                {
                  "size": "small",
                  "position": 0,
                  "hasAd": true,
                  "hasExcerpt": true
                }
              ]
            }
          ]
        }"#;

        let layout: Layout = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            layout,
            Layout {
                name: "4-medium-small-1".to_string(),
                responsive_layouts: vec![ResponsiveLayout {
                    column_count: 1,
                    tiles: vec![Tile {
                        size: "small".to_string(),
                        position: 0,
                        has_ad: true,
                        has_excerpt: true
                    }]
                }]
            }
        );
    }
    #[test]
    fn test_section_settings_deserialization() {
        let json_data = r#"
        {
          "sectionId": "sports",
          "isFollowed": true,
          "isBlocked": true
        }"#;

        let section: SectionSettings = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            section,
            SectionSettings {
                section_id: "sports".to_string(),
                is_blocked: true,
                is_followed: true
            }
        );
    }

    #[test]
    fn test_fakespot_feed_deserialization() {
        let json_data = r#"
        {
          "products": [
            {
              "id": "fakespot",
              "title": "Fakespot Product",
              "category": "News",
              "imageUrl": "https://example.com",
              "url": "https://example.com"
            }
          ],
          "defaultCategoryName": "Fakespot Cat",
          "headerCopy": "Fakespot by Mozilla",
          "footerCopy": "Blurb suitable for a footer",
          "cta": {
            "ctaCopy": "Fakespot blurb",
            "url": "https://example.com"
            }
        }"#;

        let fakespot_feed: FakespotFeed = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            fakespot_feed,
            FakespotFeed {
                products: vec![FakespotProduct {
                    id: "fakespot".to_string(),
                    title: "Fakespot Product".to_string(),
                    category: "News".to_string(),
                    image_url: Url::parse("https://example.com").unwrap(),
                    url: Url::parse("https://example.com").unwrap()
                }],
                default_category_name: "Fakespot Cat".to_string(),
                header_copy: "Fakespot by Mozilla".to_string(),
                footer_copy: "Blurb suitable for a footer".to_string(),
                cta: FakespotCta {
                    cta_copy: "Fakespot blurb".to_string(),
                    url: Url::parse("https://example.com").unwrap()
                }
            }
        );
    }

    #[test]
    fn test_feed_section_deserialization() {
        let json_data = r#"
        {
 "receivedFeedRank": 0,
      "recommendations": [
        {
          "corpusItemId": "17538d96-71dc-4196-bb2a-968cddc15474",
          "scheduledCorpusItemId": "5f72b12c-2723-470a-a51d-6fe88aa555d7",
          "url": "https://example.com/",
          "title": "Title for Rec",
          "excerpt": "Blurb about the rec",
          "topic": "business",
          "publisher": "Moz",
          "isTimeSensitive": true,
          "imageUrl": "https://example.com/",
          "tileId": 10000000,
          "receivedRank": 0
        }
      ],
      "title": "Feed Title",
      "subtitle": "Feed Subtitle",
      "layout": {
        "name": "6-small-medium-1",
        "responsiveLayouts": [
          {
            "columnCount": 1,
            "tiles": [
              {
                "size": "small",
                "position": 0,
                "hasAd": true,
                "hasExcerpt": true
              }
            ]
          }
        ]
      },
      "isFollowed": false,
      "isBlocked": false
        }"#;

        let feed_section: FeedSection = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            feed_section,
            FeedSection {
                received_feed_rank: 0,
                recommendations: vec![ReccomendationDataItem {
                    corpus_item_id: Uuid::parse_str("17538d96-71dc-4196-bb2a-968cddc15474")
                        .unwrap(),
                    schdeuled_corpus_item_id: Uuid::parse_str(
                        "5f72b12c-2723-470a-a51d-6fe88aa555d7"
                    )
                    .unwrap(),
                    url: Url::parse("https://example.com").unwrap(),
                    title: "Title for Rec".to_string(),
                    excerpt: "Blurb about the rec".to_string(),
                    topic: Some("business".to_string()),
                    publisher: "Moz".to_string(),
                    is_time_sensitive: true,
                    image_url: Url::parse("https://example.com").unwrap(),
                    tile_id: 10000000,
                    received_rank: 0
                }],
                title: "Feed Title".to_string(),
                subtitle: Some("Feed Subtitle".to_string()),
                layout: Layout {
                    name: "6-small-medium-1".to_string(),
                    responsive_layouts: vec![ResponsiveLayout {
                        column_count: 1,
                        tiles: vec![Tile {
                            size: "small".to_string(),
                            position: 0,
                            has_ad: true,
                            has_excerpt: true
                        }]
                    }]
                },
                is_followed: false,
                is_blocked: false
            }
        );
    }

    #[test]
    fn test_feeds_deserialization() {
        let json_data = r#"
        {
    "need_to_know": {
      "recommendations": [
        {
          "corpusItemId": "17538d96-71dc-4196-bb2a-968cddc15474",
          "scheduledCorpusItemId": "5f72b12c-2723-470a-a51d-6fe88aa555d7",
          "url": "https://example.com/",
          "title": "Need to Know Rec Title",
          "excerpt": "Need to Know Excerpt",
          "topic": "business",
          "publisher": "Mozilla",
          "isTimeSensitive": true,
          "imageUrl": "https://example.com/",
          "tileId": 10000000,
          "receivedRank": 0
        }
      ],
      "title": "recommendations_bucket_title"
    },
    "fakespot": {
          "products": [
            {
              "id": "fakespot",
              "title": "Fakespot Product",
              "category": "News",
              "imageUrl": "https://example.com",
              "url": "https://example.com"
            }
          ],
          "defaultCategoryName": "Fakespot Cat",
          "headerCopy": "Fakespot by Mozilla",
          "footerCopy": "Fakespot blurb for footer",
          "cta": {
            "ctaCopy": "Fakespot blurb",
            "url": "https://example.com"
          }
    },
    
    "education-science": {
      "receivedFeedRank": 0,
      "recommendations": [
        {
          "corpusItemId": "17538d96-71dc-4196-bb2a-968cddc15474",
          "scheduledCorpusItemId": "5f72b12c-2723-470a-a51d-6fe88aa555d7",
          "url": "https://example.com/",
          "title": "Education Science Title",
          "excerpt": "Education Science Excerpt",
          "topic": "education",
          "publisher": "Mozilla",
          "isTimeSensitive": true,
          "imageUrl": "https://example.com/",
          "tileId": 10000000,
          "receivedRank": 0
        }
      ],
      "title": "Title",
      "subtitle": "Subtitle",
      "layout": {
        "name": "3-small-1-medium",
        "responsiveLayouts": [
          {
            "columnCount": 1,
            "tiles": [
              {
                "size": "small",
                "position": 0,
                "hasAd": true,
                "hasExcerpt": true
              }
            ]
          }
        ]
      },
      "isFollowed": false,
      "isBlocked": false
    }
  }"#;

        let feeds: Feeds = serde_json::from_str(json_data).unwrap();
        assert_eq!(
            feeds,
            Feeds {
                need_to_know: Some(CuratedRecommendationsBucket {
                    recommendations: vec![ReccomendationDataItem {
                        corpus_item_id: Uuid::parse_str("17538d96-71dc-4196-bb2a-968cddc15474")
                            .unwrap(),
                        schdeuled_corpus_item_id: Uuid::parse_str(
                            "5f72b12c-2723-470a-a51d-6fe88aa555d7"
                        )
                        .unwrap(),
                        url: Url::parse("https://example.com").unwrap(),
                        title: "Need to Know Rec Title".to_string(),
                        excerpt: "Need to Know Excerpt".to_string(),
                        topic: Some("business".to_string()),
                        publisher: "Mozilla".to_string(),
                        is_time_sensitive: true,
                        image_url: Url::parse("https://example.com").unwrap(),
                        tile_id: 10000000,
                        received_rank: 0
                    }],
                    title: Some("recommendations_bucket_title".to_string())
                }),
                fakespot: Some(FakespotFeed {
                    products: vec![FakespotProduct {
                        id: "fakespot".to_string(),
                        title: "Fakespot Product".to_string(),
                        category: "News".to_string(),
                        image_url: Url::parse("https://example.com").unwrap(),
                        url: Url::parse("https://example.com").unwrap()
                    }],
                    default_category_name: "Fakespot Cat".to_string(),
                    header_copy: "Fakespot by Mozilla".to_string(),
                    footer_copy: "Fakespot blurb for footer".to_string(),
                    cta: FakespotCta {
                        cta_copy: "Fakespot blurb".to_string(),
                        url: Url::parse("https://example.com").unwrap()
                    }
                }),
                top_stories_section: None,
                business: None,
                career: None,
                arts: None,
                food: None,
                health: None,
                home: None,
                finance: None,
                government: None,
                sports: None,
                tech: None,
                travel: None,
                education: None,
                hobbies: None,
                society_parenting: None,
                society: None,
                education_science: Some(FeedSection {
                    received_feed_rank: 0,
                    recommendations: vec![ReccomendationDataItem {
                        corpus_item_id: Uuid::parse_str("17538d96-71dc-4196-bb2a-968cddc15474")
                            .unwrap(),
                        schdeuled_corpus_item_id: Uuid::parse_str(
                            "5f72b12c-2723-470a-a51d-6fe88aa555d7"
                        )
                        .unwrap(),
                        url: Url::parse("https://example.com").unwrap(),
                        title: "Education Science Title".to_string(),
                        excerpt: "Education Science Excerpt".to_string(),
                        topic: Some("education".to_string()),
                        publisher: "Mozilla".to_string(),
                        is_time_sensitive: true,
                        image_url: Url::parse("https://example.com").unwrap(),
                        tile_id: 10000000,
                        received_rank: 0
                    }],
                    title: "Title".to_string(),
                    subtitle: Some("Subtitle".to_string()),
                    layout: Layout {
                        name: "3-small-1-medium".to_string(),
                        responsive_layouts: vec![ResponsiveLayout {
                            column_count: 1,
                            tiles: vec![Tile {
                                size: "small".to_string(),
                                position: 0,
                                has_ad: true,
                                has_excerpt: true
                            }]
                        }]
                    },
                    is_followed: false,
                    is_blocked: false
                })
            }
        );
    }

    #[test]
    fn test_reccomendation_response_deserialization() {
        let json_data = r#"
        {
        "recommendedAt": 0,
        "data": [
        {
          "corpusItemId": "17538d96-71dc-4196-bb2a-968cddc15474",
          "scheduledCorpusItemId": "5f72b12c-2723-470a-a51d-6fe88aa555d7",
          "url": "https://example.com/",
          "title": "Rec Title",
          "excerpt": "Rec Excerpt",
          "topic": "business",
          "publisher": "Mozilla",
          "isTimeSensitive": true,
          "imageUrl": "https://example.com/",
          "tileId": 10000000,
          "receivedRank": 0
        }
      ]
  }"#;

        let reccomendation_response: CuratedRecommendationsResponse =
            serde_json::from_str(json_data).unwrap();
        assert_eq!(
            reccomendation_response,
            CuratedRecommendationsResponse {
                recommended_at: 0,
                data: vec![ReccomendationDataItem {
                    corpus_item_id: Uuid::parse_str("17538d96-71dc-4196-bb2a-968cddc15474")
                        .unwrap(),
                    schdeuled_corpus_item_id: Uuid::parse_str(
                        "5f72b12c-2723-470a-a51d-6fe88aa555d7"
                    )
                    .unwrap(),
                    url: Url::parse("https://example.com").unwrap(),
                    title: "Rec Title".to_string(),
                    excerpt: "Rec Excerpt".to_string(),
                    topic: Some("business".to_string()),
                    publisher: "Mozilla".to_string(),
                    is_time_sensitive: true,
                    image_url: Url::parse("https://example.com").unwrap(),
                    tile_id: 10000000,
                    received_rank: 0
                }],
                feeds: None
            }
        );
    }
}
