use chrono::Utc;
use clap::{Subcommand, Parser};
use log::info;
use smarther::model::{SetStatusRequest, ThermostatMode, ThermostatFunction, Measurement, ProgramIdentifier};

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
    #[clap(name = "status")]
    GetStatus {
        #[clap(name = "PLANT_ID")]
        plant_id: String,
        #[clap(name = "MODULE_ID")]
        module_id: String,
    },
    #[clap(name = "boost")]
    Boost {
        #[clap(name = "PLANT_ID")]
        plant_id: String,
        #[clap(name = "MODULE_ID")]
        module_id: String,
        #[clap(name = "DURATION")]
        duration: i64,
    },
    #[clap(name = "off")]
    Off {
        #[clap(name = "PLANT_ID")]
        plant_id: String,
        #[clap(name = "MODULE_ID")]
        module_id: String,
    },
    #[clap(name = "manual")]
    Manual {
        #[clap(name = "PLANT_ID")]
        plant_id: String,
        #[clap(name = "MODULE_ID")]
        module_id: String,
        #[clap(name = "TEMPERATURE")]
        temperature: f32,
    },
    #[clap(name = "program")]
    Program {
        #[clap(name = "PLANT_ID")]
        plant_id: String,
        #[clap(name = "MODULE_ID")]
        module_id: String,
        #[clap(name = "PROGRAM_NUMBERS")]
        program_numbers: Vec<u32>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    let client = smarther::SmartherApi::default();
    let auth_file = args.auth_file.unwrap_or_else(|| "saved_tokens.json".into());

    let auth_info = match std::fs::read_to_string(&auth_file) {
        Ok(content) => {
            let auth_info: smarther::AuthorizationInfo = serde_json::from_str(&content)?;
            Some(auth_info)
        },
        Err(_) => None
    };

    if let Commands::Tokens { client_id, client_secret, subkey } = args.command {
        let access_token = client.get_oauth_access_code(&client_id, &client_secret, None, &subkey).await?;
        let refreshed_token = client.refresh_token(&access_token).await?;
        let token_file_content = serde_json::to_string_pretty(&refreshed_token)?;
        info!("{}", token_file_content);
        std::fs::write(auth_file, token_file_content)?;
        return Ok(());
    }

    let mut auth_info = auth_info.expect("Missing authentication file, try to use the tokens subcommand first");

    if auth_info.is_refresh_needed() {
        let refreshed_token = client.refresh_token(&auth_info).await?;
        let token_file_content = serde_json::to_string_pretty(&refreshed_token)?;
        std::fs::write(auth_file, token_file_content)?;
        auth_info = refreshed_token;
    }

    let client = client.with_authorization(auth_info)?;

    match args.command {
        Commands::GetPlants => {
            let plants = client.get_plants().await?;
            info!("{:#?}", plants);
        },
        Commands::GetTopology { plant_id } => {
            let topology = client.get_topology(&plant_id).await?;
            info!("{:#?}", topology);
        },
        Commands::GetStatus { plant_id, module_id } => {
            let status = client.get_device_status(&plant_id, &module_id).await?;
            info!("{:#?}", status);
        },
        Commands::Boost { plant_id, module_id, duration } => {
            let activation_time = Utc::now() + chrono::Duration::minutes(duration);
            let request = SetStatusRequest {
                mode: ThermostatMode::Boost,
                function: ThermostatFunction::Heating,
                set_point: None,
                programs: None,
                activation_time: Some(activation_time.format("%FT%TZ").to_string()),
            };
            info!("{}", serde_json::to_string_pretty(&request)?);
            client.set_device_status(&plant_id, &module_id, request).await?;
        },
        Commands::Off { plant_id, module_id } => {
            let request = SetStatusRequest {
                mode: ThermostatMode::Off,
                function: ThermostatFunction::Heating,
                set_point: None,
                programs: None,
                activation_time: None,
            };
            info!("{}", serde_json::to_string_pretty(&request)?);
            client.set_device_status(&plant_id, &module_id, request).await?;
        },
        Commands::Manual { plant_id, module_id, temperature } => {
            let request = SetStatusRequest {
                mode: ThermostatMode::Manual,
                function: ThermostatFunction::Heating,
                set_point: Some(Measurement::Celsius(temperature)),
                programs: None,
                activation_time: None,
            };
            info!("{}", serde_json::to_string_pretty(&request)?);
            client.set_device_status(&plant_id, &module_id, request).await?;
        },
        Commands::Program { plant_id, module_id, program_numbers } => {
            let request = SetStatusRequest {
                mode: ThermostatMode::Automatic,
                function: ThermostatFunction::Heating,
                set_point: None,
                programs: Some(program_numbers.iter().map(|n| ProgramIdentifier { number: *n}).collect()),
                activation_time: None,
            };
            info!("{}", serde_json::to_string_pretty(&request)?);
            client.set_device_status(&plant_id, &module_id, request).await?;
        },
        _ => {}
    }

    Ok(())
}