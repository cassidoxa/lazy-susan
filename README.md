# Overview

lazy-susan is a small content storage and retrieval service for my personal site, [cassidymoen.com](https://cassidymoen.com). It's currently very bare-bones, but anyone is free to try it out.

# Setup

This service requires Cargo, OpenSSL dev packages, some common build tools like pkg-config and make, and a Postgresql database. First, make a copy of `.env.template` named `.env`, create your database, and add your database URL. lazy-susan also requires a SHA-256 hashed key in the environment at `LS_API_KEY`. `LS_ADDRESS` is the base address for blog post URLS after which a posts's slug comes in the URL (e.g `https://cassidymoen.com/blog/[slug]`.)

Next we have to run our database migrations and generate our Rust types. This is done with the following commands:

1. `cargo install sea-orm-cli`
2. `sea-orm-cli migrate up`
3. `sea-orm-cli generate entity --with-serde both --date-time-crate chrono --serde-skip-hidden-column --with-copy-enums --with-prelude none --serde-skip-deserializing-primary-key -o src/entity`

We have to manually populate our `blog_metadata` table with a single row as follows:
```
    title: text
    blog_url: text (base URL where posts are served)
    syndication_url: text (URL where Atom XML document is served)
    last_updated: timestamp with timezone (RFC 3339)
    author: text
    author_email: text (optional)
    author_url: text (optional)
``` 

Finally, we can build with `cargo build --release`, optionally setting `RUSTFLAGS="-C target-cpu=native"` (or replacing native with your CPU's architecture) to potentially use SIMD when parsing markdown.

# Usage

The service offeres several endpoints:

## GET /api/posts

Returns a sorted array of information about every blog post in the database with the following type:
```
    title: string
    slug: string (used to request individual posts)
    date: string (RFC 3339 format)
    visible: boolean (deleting posts sets this flag)
```

## GET /api/posts/[slug]
```
    id: integer (database id)
    title: string
    slug: string
    blog_title: string
    author: string
    text: string (HTML rendered from markdown on response)
    tags: string[] (optional array of tags)
    date: string (RFC 3339)
    last_updated: string (RFC 3339)
    visible: boolean
    edited: boolean
```

## POST /api/posts
For publishing posts. Request should have API key in header at key "Authorization" and be in the following format:
    
```
    id: integer (database id)
    title: string
    slug: string
    blog_title: string
    author: string
    text: string (HTML rendered from markdown on response)
    tags: string[] (optional array of tags)
    date: string (RFC 3339)
    last_updated: string (RFC 3339)
    visible: boolean
    edited: boolean
```

## PUT /api/posts/[slug]
For editing posts. Request should have API key in header at key "Authorization". Request can optionally have any of the following fields:

```
    title: string
    text: string
    tags: string[]
    visible: boolean
```

## DELETE /api/posts/[slug]
Deletes post with supplied slug. Requires API key in header at key "Authorization".

## GET /api/atom
Returns and XML document with an Atom feed of all currently visible blog posts.
