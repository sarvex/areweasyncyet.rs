use crate::data::input::InputData;
use crate::data::output::OutputData;
use crate::fetcher::IssueData;
use crate::page_gen::PageGenData;
use crate::query::{GitHubQuery, Repo};
use futures_util::future::try_join;
use lazy_static::lazy_static;
use semver::Version;
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

mod data;
mod fetcher;
mod page_gen;
mod posts;
mod query;

const DATA_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data.yml");
const POSTS_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/posts.yml");
const CACHE_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/cache.json");

lazy_static! {
    static ref OUT_DIR: &'static Path = Path::new("out");
    static ref RFC_REPO: Repo = Repo::new("rust-lang", "rfcs");
    static ref RUSTC_REPO: Repo = Repo::new("rust-lang", "rust");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenv::dotenv();
    env_logger::init();
    let token = env::var("GITHUB_TOKEN")?;
    let client = reqwest::Client::new();
    let query = GitHubQuery::new(&client, &token);
    let data = load_page_gen_data(&query).await?;

    // Generate page
    if OUT_DIR.is_dir() {
        clear_dir(&*OUT_DIR)?;
    } else {
        fs::create_dir_all(&*OUT_DIR)?;
    }
    page_gen::generate(&data)?;
    copy_static_files()?;
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/CNAME"),
        OUT_DIR.join("CNAME"),
    )?;
    Ok(())
}

async fn load_page_gen_data(query: &GitHubQuery<'_>) -> Result<PageGenData, Box<dyn Error>> {
    let input_data = InputData::from_file(DATA_FILE)?;
    let fetch_list = input_data.get_fetch_list();

    let mut issue_data = IssueData::from_file(CACHE_FILE).unwrap_or_default();
    let (latest_tag, _) = try_join(
        query.query_latest_tag(&*RUSTC_REPO),
        issue_data.fetch_data(query, &fetch_list),
    )
    .await?;
    issue_data.store_to_file(CACHE_FILE)?;

    let stabilized_version = Version::new(1, 39, 0);
    let latest_stable = Version::parse(&latest_tag)?;
    let output_data = OutputData::from_input(input_data, &issue_data, &latest_stable);

    Ok(PageGenData {
        is_stable: latest_stable >= stabilized_version,
        items: output_data.0,
        posts: posts::load_posts()?,
    })
}

fn clear_dir(dir: &Path) -> io::Result<()> {
    for entry in dir.read_dir()? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            fs::remove_file(&entry.path())?;
        } else if file_type.is_dir() {
            fs::remove_dir_all(&entry.path())?;
        } else {
            unreachable!("unknown file type");
        }
    }
    Ok(())
}

fn copy_static_files() -> io::Result<()> {
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    copy_dir(src.as_ref(), OUT_DIR.as_ref())
}

fn copy_dir(src: &Path, dest: &Path) -> io::Result<()> {
    for entry in src.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            fs::copy(path, dest.join(file_name))?;
        } else if file_type.is_dir() {
            let dest_dir = dest.join(file_name);
            fs::create_dir(&dest_dir)?;
            copy_dir(&path, &dest_dir)?;
        } else {
            unreachable!("unknown file type");
        }
    }
    Ok(())
}
