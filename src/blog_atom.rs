use anyhow::anyhow;
use atom_syndication::{extension::ExtensionMap, Content, Entry, Feed, FeedBuilder, Link, Person};
use chrono::Utc;
use pulldown_cmark::{html::push_html, Options, Parser};
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};

use crate::entity::blog_metadata::Entity as BlogMetaEntity;
use crate::entity::blog_posts::{
    Column as BlogPostColumn, Entity as BlogPostEntity, Model as BlogPost,
};
use crate::{BoxResult, BASE_URL};

/// Generates new Atom feed. Run on write operations for the blog. Depends on the database
/// having a single-row "blog_metadata" table for now.
pub(crate) async fn generate_atom_feed(db: &DatabaseConnection) -> BoxResult<Feed> {
    let maybe_blog_metadata = match BlogMetaEntity::find().one(db).await {
        Ok(m) => m,
        Err(e) => return Err(Box::new(e)),
    };
    let blog_metadata = match maybe_blog_metadata {
        Some(m) => m,
        None => return Err(anyhow!("Blog metadata not in database.").into()),
    };
    let author = Person {
        name: blog_metadata.author.clone(),
        email: blog_metadata.author_email.clone(),
        uri: blog_metadata.author_url.clone(),
    };
    let posts_vec = BlogPostEntity::find()
        .order_by_desc(BlogPostColumn::Date)
        .all(db)
        .await?
        .into_iter()
        .filter(|p| p.visible);
    let self_link = Link {
        href: blog_metadata.syndication_url.clone(),
        rel: "self".to_string(),
        hreflang: Some("English".to_string()),
        mime_type: Some("application/atom+xml".to_string()),
        title: None,
        length: None,
    };
    let last_updated = Utc::now();
    let entries: Vec<Entry> = posts_vec.map(Entry::from).collect();
    let mut feed_builder = FeedBuilder::default();
    let feed = feed_builder
        .author(author)
        .lang("English".to_string())
        .link(self_link)
        .updated(last_updated)
        .entries(entries)
        .build();

    Ok(feed)
}

pub(crate) fn get_markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_SUBSCRIPT);
    options.insert(Options::ENABLE_SUPERSCRIPT);

    options
}

impl From<BlogPost> for Entry {
    fn from(p: BlogPost) -> Self {
        let post_url = format!("{}{}", BASE_URL.get().unwrap(), &p.slug);
        let author = Person {
            name: p.author.clone(),
            email: None,
            uri: None,
        };
        let link = Link {
            href: post_url.clone(),
            ..Default::default()
        };
        let md_options = get_markdown_options();
        let mut parsed_html = String::with_capacity(2048);
        let parser = Parser::new_ext(&p.text, md_options);
        push_html(&mut parsed_html, parser);
        let mut content = Content::default();
        content.set_content_type("text/html".to_string());
        content.set_value(parsed_html);

        Entry {
            title: p.title.clone().into(),
            id: post_url,
            updated: p.last_updated,
            authors: vec![author],
            categories: Vec::new(),
            contributors: Vec::new(),
            links: vec![link],
            published: Some(p.date),
            rights: None,
            source: None,
            summary: None,
            content: Some(content),
            extensions: ExtensionMap::new(),
        }
    }
}
