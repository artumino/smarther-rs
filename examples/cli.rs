use clap::{Subcommand, Parser};

#[derive(Parser)]
struct CliArgs {
    #[clap(short, long)]
    auth_file: Option<String>,
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[clap(name = "tokens")]
    Tokens {
        client_id: String,
        client_secret: String,
        subkey: String,
    },
    #[clap(name = "plants")]
    GetPlants,
    #[clap(name = "topology")]
    GetTopology {
        #[clap(name = "PLANT_ID")]
        plant_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    let client = legrand_smarther_rs::SmartherApi::default();
    let auth_file = args.auth_file.unwrap_or_else(|| "saved_tokens.json".into());

    let auth_info = match std::fs::read_to_string(&auth_file) {
        Ok(content) => {
            let auth_info: legrand_smarther_rs::AuthorizationInfo = serde_json::from_str(&content)?;
            Some(auth_info)
        },
        Err(_) => None
    };

    if let Commands::Tokens { client_id, client_secret, subkey } = args.command {
        let client = client.authorize_oauth(&client_id, &client_secret, None, &subkey).await?;
        let token_file_content = serde_json::to_string_pretty(&client.auth_info())?;
        println!("{}", token_file_content);
        std::fs::write(auth_file, token_file_content)?;
        return Ok(());
    }

    let auth_info = auth_info.expect("Missing authentication file, try to use the tokens subcommand first");
    let client = client.authorize(auth_info).await?;

    match args.command {
        Commands::GetPlants => {
            let plants = client.get_plants().await?;
            println!("{:#?}", plants);
        },
        Commands::GetTopology { plant_id } => {
            let topology = client.get_topology(&plant_id).await?;
            println!("{:#?}", topology);
        },
        _ => {}
    }

    Ok(())
}