use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::error::Error;
use std::{fs, path};
use std::fs::File;
use std::io::{copy, Write};
use structopt::StructOpt;
use indicatif::{ProgressBar,ProgressStyle};
use chrono::Utc;
use std::io::{self, Read, BufReader};
use sha2::{Sha256, Digest};


#[derive(Serialize, Deserialize)]
struct Metadata {
    urn: String,
    datetime: String,
}

#[derive(StructOpt)]
struct Cli {
    /// The URN to the model
    #[structopt(short, long)]
    urn: Option<String>,

    /// The Bearer token for authentication
    #[structopt(short, long)]
    token: String,

    #[structopt(long, parse(from_os_str))]
    update: Option<std::path::PathBuf>,

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

    // Token is always required
    let token = args.token.as_str();

    if let Some(metadata_path) = args.update {
        println!("Update flag detected. Processing metadata...");

        let metadata: Metadata = read_metadata(&metadata_path)?;
        println!("Metadata URN: {}", metadata.urn);

        // Fetch model information for the URN from metadata
        let model_version = download_model_info(&metadata.urn).await?;
        let target_file = check_and_update_file(&model_version, &metadata, token).await?;
        println!("File is up-to-date: {:?}", target_file);
        return Ok(());
    }

    // When not updating, urn is required
    let urn = match args.urn {
        Some(urn) => urn,
        None => {
            eprintln!("URN is required when not using the update flag");
            return Err("Missing required URN parameter".into());
        }
    };

    // Parse the provided URN
    let version: ModelVersion = download_model_info(&urn).await?;
    // Get the download URL from files
    if version.files.is_empty() {
        eprintln!("No files available for download");
        return Ok(());
    }

    let file = &version.files[0];
    let download_url = &file.downloadUrl;

    download_file(download_url, token, &urn, &file.name).await?;

    Ok(())
}

async fn check_and_update_file(model_version: &ModelVersion, metadata: &Metadata, token: &str)
    -> Result<(), Box<dyn std::error::Error>> {
    for file in &model_version.files {
        // Check if the file exists locally and its hash matches
        let file_path = Path::new(&file.name);
        if file_path.exists() {
            let existing_sha256 = calculate_sha256(file_path)?;
            println!("Existing SHA256: {}", existing_sha256.to_lowercase());
            println!("File SHA256: {}", file.hashes.SHA256.to_lowercase());
            if existing_sha256.to_lowercase() == file.hashes.SHA256.to_lowercase() {
                println!("File {} is up to date.", file.name);
                continue;
            } else {
                println!("File {} has a mismatching hash. Updating...", file.name);
            }
        } else {
            println!("File {} does not exist. Downloading...", file.name);
        }

        // Download and replace the file if necessary
        download_file(&file.downloadUrl, token, &metadata.urn, &file.name).await.expect("TODO: panic message");
    }

    Ok(())

}

fn calculate_sha256(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    // Open the file
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);

    // Create a SHA256 hasher
    let mut hasher = Sha256::new();

    // Read the file in chunks and update the hasher
    let mut buffer = [0; 1024]; // 1KB buffer
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    // Get the hash result and convert to hex string
    let hash = hasher.finalize();
    let hash_string = format!("{:x}", hash);

    Ok(hash_string)
}

fn read_metadata(path: &Path) -> Result<Metadata, Box<dyn Error>> {
    let file_content = fs::read_to_string(path);
    let metadata: Metadata = serde_json::from_str(&file_content.unwrap().to_string())?;
    Ok(metadata)
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
