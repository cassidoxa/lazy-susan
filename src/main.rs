use std::{
    env,
    net::SocketAddr,
    pin::Pin,
    str::FromStr,
    sync::{Arc, OnceLock, RwLock},
};

use atom_syndication::Feed;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::Service,
    Request, Response,
};
use hyper_util::rt::TokioIo;
use log::error;
use sea_orm::{Database, DatabaseConnection, EntityTrait};
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;
use tokio::net::TcpListener;

mod blog_atom;
mod blog_service;
mod entity;
mod server;

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type BoxResult<T> = std::result::Result<T, GenericError>;
type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
type PinnedServiceFuture = Pin<
    Box<
        dyn Future<Output = Result<Response<BoxBody>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
>;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;
static SERVER_API_KEY: OnceLock<String> = OnceLock::new();
static BASE_URL: OnceLock<String> = OnceLock::new();

#[tokio::main(worker_threads = 2)]
async fn main() -> BoxResult<()> {
    let (db_conn, atom_feed, listener) = initialize_service().await?;
    let context = Context {
        atom_feed: Arc::new(RwLock::new(atom_feed)),
        db: Arc::new(db_conn),
    };
    let service = LazySusanService { ctx: context };
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let service = service.clone();
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                error!("{}", e);
            }
        });
    }
}

async fn initialize_service() -> BoxResult<(DatabaseConnection, Feed, TcpListener)> {
    use crate::entity::rss_feeds::Entity as RssFeedEntity;

    env_logger::init();
    dotenvy::dotenv().expect("Expected .env file in lazy_susan directory");

    let api_key = env::var("LS_API_KEY").expect("Expected LS_API_KEY variable in environment.");
    let base_url = env::var("LS_BASE_URL").expect("Expected LS_BASE_URL variable in environment.");
    let db_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL variable in environment.");
    let ls_address = env::var("LS_ADDRESS").expect("Expected LS_ADDRESS variable in environment.");
    let ls_port = env::var("LS_PORT").expect("Expected LS_PORT variable in environment.");
    SERVER_API_KEY
        .set(api_key)
        .expect("Error writing SERVER_API_KEY");
    BASE_URL.set(base_url).expect("Error writing BASE_URL");

    let db_conn = Database::connect(db_url).await?;
    let addr_string = format!("{ls_address}:{ls_port}");
    let addr: SocketAddr = addr_string.parse().unwrap();
    let listener = TcpListener::bind(&addr).await?;

    // Assumes single blog feed used by Lazy Susan.
    let atom_string = RssFeedEntity::find_by_id(1)
        .one(&db_conn)
        .await?
        .map_or("".to_owned(), |v| v.rss_xml_string.to_owned());
    let atom_feed = Feed::from_str(&atom_string).unwrap_or_default();

    Ok((db_conn, atom_feed, listener))
}

#[derive(Debug, Clone)]
struct Context {
    atom_feed: Arc<RwLock<Feed>>,
    db: Arc<DatabaseConnection>,
}

#[derive(Debug, Clone)]
struct LazySusanService {
    ctx: Context,
}

impl Service<Request<Incoming>> for LazySusanService {
    type Response = Response<BoxBody>;
    type Error = GenericError;
    type Future = PinnedServiceFuture;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        Box::pin(blog_service::handle_request(req, self.ctx.clone()))
    }
}
