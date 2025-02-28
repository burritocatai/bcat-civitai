use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::error::Error;
use std::fs::File;
use std::io::{copy, Write};
use structopt::StructOpt;
use indicatif::{ProgressBar,ProgressStyle};
use chrono::Utc;


#[derive(Serialize)]
struct Metadata {
    urn: String,
    datetime: String,
}

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

    let client = reqwest::Client::new();
    let urn = args.urn.as_str();
    let token = args.token.as_str();

    // Parse the provided URN
    let version: ModelVersion = download_model_info(urn).await?;
    // Get the download URL from files
    if version.files.is_empty() {
        eprintln!("No files available for download");
        return Ok(());
    }

    let file = &version.files[0];
    let download_url = &file.downloadUrl;

    download_file(download_url, token, urn, &file.name).await?;

    Ok(())
}

async fn download_model_info(urn: &str) -> Result<ModelVersion, Box<dyn Error>> {
    // Parse the provided URN
    let urn_parts: Vec<&str> = urn.split(':').collect();
    if urn_parts.len() != 6 || !urn.contains('@') {
        return Err("Invalid URN format".into());
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
        return Err(format!("Failed to fetch model metadata: {}", response.status()).into());
    }

    let model: Model = response.json().await?;

    // Select the correct version
    let version = model
        .modelVersions
        .into_iter()
        .find(|v| v.id == version_id)
        .ok_or("Version not found")?;

    Ok(version)
}

async fn download_file(download_url: &str, token: &str, urn: &str, file_name: &str) -> Result<(), Box<dyn Error>> {

    println!("Downloading file from: {}", download_url);
    let client = reqwest::Client::new();

    // Download the file
    let mut headers = HeaderMap::new();
    let token_value = format!("Bearer {}", token);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&token_value)?);

    let mut response = client.get(download_url).headers(headers).send().await?;
    if !response.status().is_success() {
        eprintln!("Failed to download file: {}", response.status());
        return Ok(());
    }


    let total_size = response
        .content_length()
        .ok_or("Failed to fetch content length")?;


    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap().progress_chars("=>-"),
    );


    let mut downloaded_file = File::create(file_name)?;
    let mut downloaded_data = 0u64;

    while let Some(chunk) = response.chunk().await? {
        downloaded_file.write_all(&chunk)?;
        downloaded_data += chunk.len() as u64;
        pb.set_position(downloaded_data);
    }

    pb.finish_with_message("Download complete!");

    let metadata = Metadata {
        urn: urn.parse().unwrap(),
        datetime: Utc::now().to_rfc3339(), // Get ISO8601 string as timestamp
    };

    let metadata_json = serde_json::to_string_pretty(&metadata)?;

    // Create metadata file name
    let metadata_file_name = format!("{}.metadata.json", file_name);

    std::fs::write(&metadata_file_name, metadata_json)?;

    println!("Metadata saved as: {}", metadata_file_name);
    println!("Model downloaded as: {}", file_name);

    return Ok(());
}
