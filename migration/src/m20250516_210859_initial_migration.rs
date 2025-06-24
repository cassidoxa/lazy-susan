//use crate::extension::postgres::Type;
use crate::{
    extension::postgres::Type,
    sea_orm::{DeriveActiveEnum, EnumIter},
};
use sea_orm_migration::{prelude::*, schema::*, sea_orm::ActiveEnum};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(Alias::new("content_type"))
                    .values([Alias::new("Blog"), Alias::new("Podcast")])
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(BlogMetadata::Table)
                    .if_not_exists()
                    .col(pk_auto(BlogMetadata::Id))
                    .col(text(BlogMetadata::Title))
                    .col(text(BlogMetadata::BlogUrl))
                    .col(text(BlogMetadata::SyndicationUrl))
                    .col(timestamp_with_time_zone(BlogMetadata::LastUpdated))
                    .col(text(BlogMetadata::Author))
                    .col(text_null(BlogMetadata::AuthorEmail))
                    .col(text_null(BlogMetadata::AuthorUrl))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(BlogPosts::Table)
                    .if_not_exists()
                    .col(pk_auto(BlogPosts::Id))
                    .col(text(BlogPosts::Title))
                    .col(text(BlogPosts::Slug))
                    .col(text(BlogPosts::BlogTitle))
                    .col(text(BlogPosts::Author))
                    .col(text(BlogPosts::Text))
                    .col(text(BlogPosts::Description))
                    .col(text_null(BlogPosts::Image))
                    .col(array_null(BlogPosts::Tags, ColumnType::Text))
                    .col(text_null(BlogPosts::Next))
                    .col(text_null(BlogPosts::Previous))
                    .col(timestamp_with_time_zone(BlogPosts::Date))
                    .col(timestamp_with_time_zone(BlogPosts::LastUpdated))
                    .col(boolean(BlogPosts::Visible))
                    .col(boolean(BlogPosts::Edited))
                    .to_owned(),
            )
            .await?;
        //manager
        //    .create_table(
        //        Table::create()
        //            .table(PodcastEpisodes::Table)
        //            .if_not_exists()
        //            .col(pk_auto(PodcastEpisodes::Id))
        //            .col(text(PodcastEpisodes::Title))
        //            .col(text(PodcastEpisodes::Slug))
        //            .col(text(PodcastEpisodes::PodcastName))
        //            .col(array_null(PodcastEpisodes::Hosts, ColumnType::Text))
        //            .col(array_null(PodcastEpisodes::Guests, ColumnType::Text))
        //            .col(text(PodcastEpisodes::AudioUrl))
        //            .col(array_null(PodcastEpisodes::Tags, ColumnType::Text))
        //            .col(integer(PodcastEpisodes::Length))
        //            .col(timestamp_with_time_zone(PodcastEpisodes::Date))
        //            .col(timestamp_with_time_zone_null(PodcastEpisodes::LastUpdated))
        //            .to_owned(),
        //    )
        //    .await?;
        manager
            .create_table(
                Table::create()
                    .table(RssFeeds::Table)
                    .if_not_exists()
                    .col(pk_auto(RssFeeds::Id))
                    .col(ColumnDef::new(RssFeeds::ContentType).custom(ContentType::name())) // use the type for a table column
                    .col(text(RssFeeds::RssXmlString))
                    .col(timestamp_with_time_zone(RssFeeds::LastUpdated))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BlogMetadata::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BlogPosts::Table).to_owned())
            .await?;
        //manager
        //    .drop_table(Table::drop().table(PodcastEpisodes::Table).to_owned())
        //    .await?;
        manager
            .drop_table(Table::drop().table(RssFeeds::Table).to_owned())
            .await?;
        manager
            .drop_type(Type::drop().name(Alias::new("content_type")).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum BlogMetadata {
    Table,
    Id,
    Title,
    BlogUrl,
    SyndicationUrl,
    LastUpdated,
    Author,
    AuthorEmail,
    AuthorUrl,
}

#[derive(DeriveIden)]
enum BlogPosts {
    Table,
    Id,
    Title,
    Slug,
    BlogTitle,
    Author,
    Text,
    Description,
    Image,
    Tags,
    Next,
    Previous,
    Date,
    LastUpdated,
    Visible,
    Edited,
}

//#[derive(DeriveIden)]
//enum Podcasts {
//    Table,
//    Id,
//    Title,
//    Author,
//    Link,
//    Description,
//    Language,
//    Explicit,
//    ImageUrl,
//    Category,
//}

//#[derive(DeriveIden)]
//enum PodcastEpisodes {
//    Table,
//    Id,
//    Title,
//    PodcastTitle,
//    Slug,
//    Hosts,
//    Guests,
//    AudioUrl,
//    Tags,
//    Length,
//    Date,
//    LastUpdated,
//}

#[derive(DeriveIden)]
enum RssFeeds {
    Table,
    Id,
    ContentType,
    RssXmlString,
    LastUpdated,
}

#[derive(EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "content_type")]
pub enum ContentType {
    #[sea_orm(string_value = "Blog")]
    Blog,
    #[sea_orm(string_value = "Podcast")]
    Podcast,
}
