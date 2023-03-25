use clap::{Subcommand, Parser};

#[derive(Parser)]
struct CliArgs {
    #[clap(short, long)]
    sub_key: String,
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[clap(name = "tokens")]
    Tokens {
        client_id: String,
        client_secret: String
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

    if let Commands::Tokens { client_id, client_secret } = args.command {
        let client = client.authorize_oauth(&client_id, &client_secret, None, &args.sub_key).await?;
        println!("{}", serde_json::to_string_pretty(&client.auth_info())?);
        return Ok(());
    }

    todo!()
}