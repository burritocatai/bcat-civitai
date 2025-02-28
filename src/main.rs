use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::io::copy;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Cli {
    /// The URN to the model
    urn: String,

    /// The Bearer token for authentication
    #[structopt(short, long)]
    token: String,
}

#[derive(Deserialize)]
struct ModelVersion {
    id: u64,
    files: Vec<ModelFile>,
}

#[derive(Deserialize)]
struct ModelFile {
    name: String,
    downloadUrl: String,
    hashes: ModelHashes,
}

#[derive(Deserialize)]
struct ModelHashes {
    SHA256: String,
}

#[derive(Deserialize)]
struct Model {
    id: u64,
    modelVersions: Vec<ModelVersion>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::from_args();

    // Parse the provided URN
    let urn_parts: Vec<&str> = args.urn.split(':').collect();
    if urn_parts.len() != 6 || !args.urn.contains('@') {
        eprintln!("Invalid URN format");
        return Ok(());
    }
    let model_type = urn_parts[2];
    let model_id_with_version: Vec<&str> = urn_parts[5].split('@').collect();
    let model_id: u64 = model_id_with_version[0].parse()?;
    let version_id: u64 = model_id_with_version[1].parse()?;

    println!("Parsed URN:");
    println!("  Model Type: {}", model_type);
    println!("  Model ID: {}", model_id);
    println!("  Version ID: {}", version_id);

    // Fetch the model metadata
    let model_url = format!("https://civitai.com/api/v1/models/{}", model_id);
    println!("Fetching model information from: {}", model_url);

    let client = reqwest::Client::new();
    let response = client.get(&model_url).send().await?;

    if !response.status().is_success() {
        eprintln!("Failed to fetch model metadata: {}", response.status());
        return Ok(());
    }

    let model: Model = response.json().await?;

    // Select the correct version
    let version = model
        .modelVersions
        .into_iter()
        .find(|v| v.id == version_id)
        .ok_or("Version not found")?;

    // println!("Found version with SHA256: {}", version.sha256);

    // Get the download URL from files
    if version.files.is_empty() {
        eprintln!("No files available for download");
        return Ok(());
    }
    let file = &version.files[0];
    let download_url = &file.downloadUrl;

    println!("Downloading file from: {}", download_url);

    // Download the file
    let mut headers = HeaderMap::new();
    let token_value = format!("Bearer {}", args.token);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&token_value)?);

    let mut response = client.get(download_url).headers(headers).send().await?;
    if !response.status().is_success() {
        eprintln!("Failed to download file: {}", response.status());
        return Ok(());
    }

    let file_name = &file.name;
    let mut downloaded_file = File::create(file_name)?;
    copy(&mut response.bytes().await?.as_ref(), &mut downloaded_file)?;

    println!("Model downloaded as: {}", file_name);

    Ok(())
}