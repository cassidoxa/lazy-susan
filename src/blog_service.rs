use chrono::{FixedOffset, TimeZone, Utc};
use http_body_util::BodyExt;
use hyper::{
    body::{Buf, Incoming},
    Method, Request, Response, StatusCode,
};
use log::error;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::blog_atom::generate_atom_feed;
use crate::entity::blog_metadata::{
    ActiveModel as BlogMetaActive, Column as BlogMetaColumn, Entity as BlogMetaEntity,
};
use crate::entity::blog_posts::{
    ActiveModel as BlogPostActive, Column as BlogPostColumn, Entity as BlogPostEntity,
    Model as BlogPost,
};
use crate::entity::rss_feeds::{
    ActiveModel as RssFeedActive, Column as RssFeedColumn, Entity as RssFeedEntity,
};
use crate::entity::sea_orm_active_enums::ContentType;
use crate::{
    server::{api_key_auth, full},
    BoxBody, BoxResult, Context, BASE_URL,
};

/// Utility struct for our GET /posts/ handler that returns a sorted collection of all blog posts.
#[derive(Deserialize, Serialize)]
struct BlogPostInfo {
    title: String,
    slug: String,
    date: String,
    visible: bool,
}

impl From<BlogPost> for BlogPostInfo {
    fn from(v: BlogPost) -> Self {
        Self {
            title: v.title,
            slug: v.slug,
            date: v.date.to_rfc3339(),
            visible: v.visible,
        }
    }
}

/// Utility struct for our blog post edit handler function.
#[derive(Deserialize)]
struct EditRequest {
    title: Option<String>,
    text: Option<String>,
    tags: Option<Vec<String>>,
    visible: Option<bool>,
}

/// Main routing function.
pub async fn handle_request(req: Request<Incoming>, ctx: Context) -> BoxResult<Response<BoxBody>> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/api/posts") => get_blog_posts(&ctx.db).await,
        (&Method::GET, path) if path.starts_with("/api/posts/") => {
            get_blog_post(&ctx.db, &req).await
        }
        (&Method::POST, "/api/posts") => write_blog_post(&ctx, req).await,
        (&Method::PUT, path) if path.starts_with("/api/posts/") => edit_blog_post(&ctx, req).await,
        (&Method::DELETE, path) if path.starts_with("/api/posts/") => {
            delete_blog_post(&ctx, req).await
        }
        (&Method::GET, "/api/atom") => get_blog_rss(&ctx).await,
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain")
            .body(full(b"Not Found".as_slice()))
            .unwrap()),
    }
}

/// Handler function for GET /posts that returns a sorted collection of all blog posts.
async fn get_blog_posts(db: &DatabaseConnection) -> BoxResult<Response<BoxBody>> {
    let posts_vec = match BlogPostEntity::find()
        .order_by_desc(BlogPostColumn::Date)
        .all(db)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    let posts_info: Vec<BlogPostInfo> = posts_vec.into_iter().map(|p| p.into()).collect();
    let json =
        serde_json::to_string(&posts_info).expect("Error converting blog post info vec to JSON");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(full(json))
        .unwrap())
}

async fn get_blog_post(
    db: &DatabaseConnection,
    req: &Request<Incoming>,
) -> BoxResult<Response<BoxBody>> {
    use crate::blog_atom::get_markdown_options;
    use pulldown_cmark::{html::push_html, Parser};

    let path_vec = &req.uri().path().split("/").collect::<Vec<&str>>();
    let slug = path_vec[3];
    if (path_vec.len() != 4) || slug.is_empty() {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/plain")
            .body(full(
                b"Bad request: URL should be in format '/api/posts/[slug]'".as_slice(),
            ))
            .unwrap());
    }
    let maybe_post = match BlogPostEntity::find()
        .filter(BlogPostColumn::Slug.eq(slug))
        .one(db)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    let mut post = match maybe_post {
        Some(p) if p.visible => p,
        _ => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(full(b"Not Found".as_slice()))
                .unwrap())
        }
    };
    let md_options = get_markdown_options();
    let mut parsed_html = String::with_capacity(2048);
    let parser = Parser::new_ext(&post.text, md_options);
    push_html(&mut parsed_html, parser);
    post.text = parsed_html;
    let json = serde_json::to_string(&post).expect("Error converting blog post to JSON");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(full(json))
        .unwrap())
}

/// Handler function for writing blog posts into the database. Authenticates, Parses request
/// JSON, checks if we're adding a duplicate (returns error if so,) writes new post data to
/// database, and updates Atom syndication XML.
async fn write_blog_post(ctx: &Context, req: Request<Incoming>) -> BoxResult<Response<BoxBody>> {
    if !api_key_auth(&req) {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "ApiKey")
            .body(full(b"Unauthorized: Requires API key".as_slice()))
            .unwrap());
    }
    let whole_body = req.collect().await?.aggregate();
    let blog_post: BlogPost = match serde_json::from_reader(whole_body.reader()) {
        Ok(b) => b,
        Err(e) => {
            error!("{}", e);
            let err_string = format!("Request contained malformed JSON: {}", e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(full(err_string))
                .unwrap());
        }
    };

    let maybe_duplicate = match BlogPostEntity::find()
        .filter(BlogPostColumn::Slug.eq(&blog_post.slug))
        .one(&*ctx.db)
        .await
    {
        Ok(d) => d,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    if maybe_duplicate.is_some() {
        return Ok(Response::builder()
            .status(StatusCode::CONFLICT)
            .body(full(b"Error: Duplicate slug/post title".as_slice()))
            .unwrap());
    }
    let blog_post_active: BlogPostActive = blog_post.into();
    let blog_post_returned = match blog_post_active.insert(&*ctx.db).await {
        Ok(b) => b,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    if let Some(r) = update_blog_rss(ctx).await {
        return Ok(r);
    };
    if let Some(r) = set_blog_updated(&ctx.db, &blog_post_returned.blog_title).await {
        return Ok(r);
    };
    let response_location = format!("{}{}", BASE_URL.get().unwrap(), &blog_post_returned.slug);

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Location", response_location)
        .body(full(b"Post successfully entered".as_slice()))
        .unwrap())
}

async fn edit_blog_post(ctx: &Context, req: Request<Incoming>) -> BoxResult<Response<BoxBody>> {
    if !api_key_auth(&req) {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "ApiKey")
            .body(full(b"Unauthorized: Requires API key".as_slice()))
            .unwrap());
    }
    let path_vec = &req.uri().path().split("/").collect::<Vec<&str>>();
    let slug = path_vec[3].to_owned();
    if (path_vec.len() != 4) || slug.is_empty() {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/plain")
            .body(full(
                b"Bad request: URL should be in format '/api/posts/[slug]'".as_slice(),
            ))
            .unwrap());
    }
    let whole_body = req.collect().await?.aggregate();
    let edits: EditRequest = match serde_json::from_reader(whole_body.reader()) {
        Ok(b) => b,
        Err(e) => {
            let err_string = format!("Request contained malformed JSON: {}", e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(full(err_string))
                .unwrap());
        }
    };
    let maybe_blog_post = match BlogPostEntity::find()
        .filter(BlogPostColumn::Slug.eq(slug))
        .one(&*ctx.db)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    let blog_post = match maybe_blog_post {
        Some(p) => p,
        None => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(full(b"Not Found".as_slice()))
                .unwrap())
        }
    };
    let mut blog_post_active: BlogPostActive = blog_post.into();
    if edits.title.is_some() {
        blog_post_active.title = Set(edits.title.unwrap().to_owned());
    }
    if edits.text.is_some() {
        blog_post_active.text = Set(edits.text.unwrap().to_owned());
    }
    if edits.tags.is_some() {
        blog_post_active.tags = Set(edits.tags.clone());
    }
    if edits.visible.is_some() {
        blog_post_active.visible = Set(edits.visible.unwrap());
    }
    blog_post_active.edited = Set(true);
    let now = FixedOffset::east_opt(0)
        .unwrap()
        .from_utc_datetime(&Utc::now().naive_utc());
    blog_post_active.last_updated = Set(now);
    let blog_post_returned = match blog_post_active.update(&*ctx.db).await {
        Ok(b) => b,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    if let Some(r) = update_blog_rss(ctx).await {
        return Ok(r);
    };
    if let Some(r) = set_blog_updated(&ctx.db, &blog_post_returned.blog_title).await {
        return Ok(r);
    };
    let success_string = format!("Post successfully edited: {}", &blog_post_returned.slug);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(full(success_string))
        .unwrap())
}

async fn delete_blog_post(ctx: &Context, req: Request<Incoming>) -> BoxResult<Response<BoxBody>> {
    if !api_key_auth(&req) {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "ApiKey")
            .body(full(b"Unauthorized: Requires API key".as_slice()))
            .unwrap());
    }
    let path_vec = &req.uri().path().split("/").collect::<Vec<&str>>();
    let slug = path_vec[3].to_owned();
    if (path_vec.len() != 4) || slug.is_empty() {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/plain")
            .body(full(
                b"Bad request: URL should be in format '/api/posts/[slug]'".as_slice(),
            ))
            .unwrap());
    }
    let mut blog_post: BlogPostActive = match BlogPostEntity::find()
        .filter(BlogPostColumn::Slug.eq(&slug))
        .one(&*ctx.db)
        .await?
    {
        Some(p) => p.into(),
        None => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(full(b"Not Found".as_slice()))
                .unwrap())
        }
    };
    blog_post.visible = Set(false);
    let blog_post_returned = match blog_post.update(&*ctx.db).await {
        Ok(p) => p,
        Err(e) => {
            error!("{}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(full(b"Database error".as_slice()))
                .unwrap());
        }
    };
    if let Some(r) = update_blog_rss(ctx).await {
        return Ok(r);
    };
    if let Some(r) = set_blog_updated(&ctx.db, &blog_post_returned.blog_title).await {
        return Ok(r);
    };
    let success_string = format!("Post successfully deleted: {}", &slug);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(full(success_string))
        .unwrap())
}

async fn update_blog_rss(ctx: &Context) -> Option<Response<BoxBody>> {
    let new_feed = match generate_atom_feed(&ctx.db).await {
        Ok(f) => f,
        Err(e) => {
            error!("{}", e);
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(b"Error generating Atom feed".as_slice()))
                    .unwrap(),
            );
        }
    };
    let atom_feed_model = match RssFeedEntity::find()
        .filter(RssFeedColumn::ContentType.eq(ContentType::Blog))
        .one(&*ctx.db)
        .await
    {
        Ok(a) => a,
        Err(e) => {
            error!("{}", e);
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(b"Database error".as_slice()))
                    .unwrap(),
            );
        }
    };
    let mut atom_feed_model: RssFeedActive = match atom_feed_model {
        Some(a) => a.into(),
        None => {
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(b"Blog Atom XML not found".as_slice()))
                    .unwrap(),
            );
        }
    };
    let now = FixedOffset::east_opt(0)
        .unwrap()
        .from_utc_datetime(&Utc::now().naive_utc());
    atom_feed_model.last_updated = Set(now);
    atom_feed_model.rss_xml_string = Set(new_feed.to_string());
    match atom_feed_model.update(&*ctx.db).await {
        Ok(_) => {}
        Err(e) => {
            error!("{}", e);
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(b"Database error".as_slice()))
                    .unwrap(),
            );
        }
    };
    {
        let mut feed = ctx.atom_feed.write().unwrap();
        *feed = new_feed;
    }

    None
}

async fn get_blog_rss(ctx: &Context) -> BoxResult<Response<BoxBody>> {
    let feed_string = {
        ctx.atom_feed
            .read()
            .expect("Error reading Atom feed RwLock")
            .to_string()
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/atom+xml")
        .body(full(feed_string))
        .unwrap())
}

async fn set_blog_updated(db: &DatabaseConnection, blog_title: &str) -> Option<Response<BoxBody>> {
    let maybe_blog_meta = match BlogMetaEntity::find()
        .filter(BlogMetaColumn::Title.eq(blog_title))
        .one(db)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            error!("{}", e);
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(b"Database error".as_slice()))
                    .unwrap(),
            );
        }
    };
    let mut blog_meta: BlogMetaActive = match maybe_blog_meta {
        Some(m) => m.into(),
        None => {
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(
                        b"Server error: Blog metadata not configured".as_slice(),
                    ))
                    .unwrap(),
            );
        }
    };
    let now = FixedOffset::east_opt(0)
        .unwrap()
        .from_utc_datetime(&Utc::now().naive_utc());
    blog_meta.last_updated = Set(now);
    let _ = match blog_meta.update(db).await {
        Ok(b) => b,
        Err(e) => {
            error!("{}", e);
            return Some(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain")
                    .body(full(b"Database error".as_slice()))
                    .unwrap(),
            );
        }
    };

    None
}
