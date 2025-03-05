use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::error::Error;
use std::{fs, path, env};
use std::fs::File;
use std::io::{copy, Write};
use structopt::StructOpt;
use indicatif::{ProgressBar,ProgressStyle};
use chrono::Utc;
use std::io::{self, Read, BufReader};
use sha2::{Sha256, Digest};
use std::path::PathBuf;

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
    #[structopt(long)]
    token: Option<String>,

    #[structopt(long, parse(from_os_str))]
    update: Option<std::path::PathBuf>,

    /// Base directory for downloads
    #[structopt(long, parse(from_os_str))]
    base_dir: Option<PathBuf>,

    /// Use ComfyUI directory structure
    #[structopt(long)]
    comfyui: bool,
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

/// Parses the URN components for file organization
struct UrnComponents {
    ecosystem: String,
    type_name: String,
    source: String,
    id: String,
    version: String,
    layer: Option<String>,
    format: Option<String>,
}

impl UrnComponents {
    fn from_urn(urn: &str) -> Result<Self, Box<dyn Error>> {
        // Parse the URN: urn:air:{ecosystem}:{type}:{source}:{id}@{version?}:{layer?}.?{format?}
        let parts: Vec<&str> = urn.split(':').collect();
        if parts.len() < 6 {
            return Err("Invalid URN format".into());
        }
        
        let ecosystem = parts[2].to_string();
        let type_name = parts[3].to_string();
        let source = parts[4].to_string();
        
        let id_version_parts: Vec<&str> = if parts[5].contains('@') {
            parts[5].split('@').collect()
        } else {
            return Err("Invalid URN format: missing version identifier".into());
        };
        
        let id = id_version_parts[0].to_string();
        let version = id_version_parts[1].to_string();
        
        // Optional layer and format
        let layer = if parts.len() > 6 {
            Some(parts[6].to_string())
        } else {
            None
        };
        
        let format = if let Some(l) = &layer {
            if l.contains('.') {
                let format_parts: Vec<&str> = l.split('.').collect();
                if format_parts.len() > 1 {
                    Some(format_parts[1].to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        
        Ok(UrnComponents {
            ecosystem,
            type_name,
            source,
            id,
            version,
            layer,
            format,
        })
    }
    
    fn get_target_path(&self, is_comfyui: bool) -> PathBuf {
        if !is_comfyui {
            // If not using ComfyUI structure, just return an empty path
            // which will place files directly in the base_dir
            return PathBuf::new();
        }

        let mut path = PathBuf::new();
        if self.ecosystem.contains("flux") && self.type_name.contains("checkpoint") {
            // place flux in unet not checkpoints
            path.push("unet")
        } 
        else {
            path.push(format!("{}s", &self.type_name));
        }
        path
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::from_args();

    // Token is always required
    let token = match args.token {
        Some(token_str) => token_str.to_string(),
        None => {
            match env::var("CIVITAI_API_TOKEN") {
                Ok(env_token) => env_token,
                Err(_) => {
                    eprintln!("Error: No authentication token provided.");
                    return Err("Missing required token parameter".into());
                }
            }
        }
    };

    let base_dir = match args.base_dir {
        Some(dir) => dir,
        None => {
            match env::var("COMFYUI_BASE_DIR") {
                Ok(env_dir) => PathBuf::from(env_dir),
                Err(_) => PathBuf::from(".")
            }
        }
    };

    // Determine if ComfyUI structure should be used
    let use_comfyui = args.comfyui || env::var("COMFYUI_BASE_DIR").is_ok();


    println!("Using base directory: {}", base_dir.display());


    if let Some(metadata_path) = args.update {
        println!("Update flag detected. Processing metadata...");

        let metadata: Metadata = read_metadata(&metadata_path)?;
        println!("Metadata URN: {}", metadata.urn);

        // Fetch model information for the URN from metadata
        let model_version = download_model_info(&metadata.urn).await?;
        let target_file = check_and_update_file(&model_version, &metadata, &token, &base_dir, use_comfyui).await?;
        println!("Files are up-to-date");
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

    download_file(download_url, &token, &urn, &file.name, &base_dir, use_comfyui).await?;

    Ok(())
}

async fn check_and_update_file(model_version: &ModelVersion, metadata: &Metadata, 
    token: &str, base_dir: &PathBuf, use_comfyui: bool)
    -> Result<(), Box<dyn std::error::Error>> {
    
    // Parse the URN to get the target path
    let urn_components = UrnComponents::from_urn(&metadata.urn)?;
    let target_path = base_dir.join(urn_components.get_target_path(use_comfyui));
    
    for file in &model_version.files {
        // Check if the file exists locally and its hash matches
        let file_path = target_path.join(&file.name);
        
        if file_path.exists() {
            let existing_sha256 = calculate_sha256(&file_path)?;
            println!("Existing SHA256: {}", existing_sha256.to_lowercase());
            println!("File SHA256: {}", file.hashes.SHA256.to_lowercase());
            if existing_sha256.to_lowercase() == file.hashes.SHA256.to_lowercase() {
                println!("File {} is up to date.", file_path.display());
                continue;
            } else {
                println!("File {} has a mismatching hash. Updating...", file_path.display());
            }
        } else {
            println!("File {} does not exist. Downloading...", file_path.display());
        }

        // Download and replace the file if necessary
        download_file(&file.downloadUrl, token, &metadata.urn, &file.name, base_dir, use_comfyui).await?;
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
    let file_content = fs::read_to_string(path)?;
    let metadata: Metadata = serde_json::from_str(&file_content)?;
    Ok(metadata)
}

async fn download_model_info(urn: &str) -> Result<ModelVersion, Box<dyn Error>> {
    // Parse the provided URN
    let urn_parts: Vec<&str> = urn.split(':').collect();
    if urn_parts.len() < 6 || !urn.contains('@') {
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

async fn download_file(download_url: &str, token: &str, urn: &str, 
    file_name: &str, base_dir: &PathBuf, use_comfyui: bool)
    -> Result<(), Box<dyn Error>> {
    println!("Downloading file from: {}", download_url);
    
    // Parse the URN to get the target path
    let urn_components = UrnComponents::from_urn(urn)?;
    let target_path = base_dir.join(urn_components.get_target_path(use_comfyui));
    
    // Create target directory if it doesn't exist
    fs::create_dir_all(&target_path)?;
    
    let file_path = target_path.join(file_name);
    let metadata_path = target_path.join(format!("{}.metadata.json", file_name));
    
    println!("Target file path: {}", file_path.display());

    let client = reqwest::Client::new();

    // Download the file
    let mut headers = HeaderMap::new();
    let token_value = format!("Bearer {}", token);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&token_value)?);

    let mut response = client.get(download_url).headers(headers).send().await?;
    if !response.status().is_success() {
        eprintln!("Failed to download file: {}", response.status());
        return Err(format!("Failed to download file: {}", response.status()).into());
    }

    let total_size = response
        .content_length()
        .ok_or("Failed to fetch content length")?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );

    let mut downloaded_file = File::create(&file_path)?;
    let mut downloaded_data = 0u64;

    while let Some(chunk) = response.chunk().await? {
        downloaded_file.write_all(&chunk)?;
        downloaded_data += chunk.len() as u64;
        pb.set_position(downloaded_data);
    }

    pb.finish_with_message("Download complete!");

    let metadata = Metadata {
        urn: urn.to_string(),
        datetime: Utc::now().to_rfc3339(), // Get ISO8601 string as timestamp
    };

    let metadata_json = serde_json::to_string_pretty(&metadata)?;

    // Write metadata file
    std::fs::write(&metadata_path, metadata_json)?;

    println!("Metadata saved as: {}", metadata_path.display());
    println!("Model downloaded as: {}", file_path.display());

    Ok(())
}