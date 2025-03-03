# bcat-civitai

A simple Rust application for downloading and updating AI models from [CivitAI](https://civitai.com/).

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- Download models directly from CivitAI using their URN
- Update existing models by checking against their SHA256 hash
- Show download progress with a sleek progress bar
- Automatically generate and store metadata for downloaded models
- Token-based authentication for accessing restricted content

## Installation

### Prerequisites

- Rust and Cargo installed on your system

### Building from source

```bash
# Clone the repository
git clone https://github.com/burritocatai/bcat-civitai.git
cd civitai-dl

# Build the application
cargo build --release

# The binary will be available at target/release/bcat-civitai
```

## Usage

### Downloading a model

```bash
bcat-civitai --urn urn:air:flux1:lora:civitai:1075055@1206817 --base_dir /comfyui/ComfyUI/models --token YOUR_CIVITAI_TOKEN
```

### Updating an existing model

```bash
bcat-civitai --token YOUR_CIVITAI_TOKEN --update path/to/model.safetensors.metadata.json
```

### Command-line options

| Option | Description |
|--------|-------------|
| `-u, --urn` | The URN to the model (e.g., `civitai:model:checkpoint:12345@67890`) |
| `-t, --token` | Your CivitAI bearer token for authentication |
| `-b, --base-dir` | Base directory for storing downloaded models, your ComfyUI directory is recommended |
| `--update` | Path to a metadata file for updating an existing model |

## URN Format

The application expects URNs in the following format:

```
urn:air:{ecosystem}:{type}:{source}:{id}@{version?}:{layer?}.?{format?}
```

Examples:
- `urn:air:flux1:lora:civitai:1075055@1206817`

## Metadata

When downloading a model, the application creates a metadata JSON file with the following format:

```json
{
  "urn": "urn:air:flux1:lora:civitai:1075055@1206817",
  "datetime": "2023-07-01T12:34:56.789Z"
}
```

This metadata is used for updating the model later.

## Authentication

You need a CivitAI bearer token to download models. You can get one from your CivitAI account settings.

## Error Handling

The application provides informative error messages for:
- Invalid URN format
- Network connection issues
- Authentication problems
- File system errors

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Acknowledgements

- [CivitAI](https://civitai.com/) for providing the model repository
- [reqwest](https://crates.io/crates/reqwest) for HTTP client functionality
- [indicatif](https://crates.io/crates/indicatif) for progress bar support
- [structopt](https://crates.io/crates/structopt) for command-line argument parsing
